//! Assembles the running app (plan §4.1, §4.3): encrypted store → capture thread (starts
//! paused, D12) → inference runtime (only if verified model files are present) → job workers.
//!
//! Nothing here downloads anything. Without model files the app still captures and searches;
//! extraction jobs stay queued until a model is set up. Without embedder files, embedding jobs
//! stay queued and search is keyword-only.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tsr_capture::engine::CaptureCore;
use tsr_capture::platform_win::{CaptureHandle, WinPlatform, spawn_capture};
use tsr_capture::sink::StoreSink;
use tsr_guard::blocklist::{Blocklist, builtin_rules};
use tsr_infer::runtime::{IdlePolicy, Runner, Runtime};
use tsr_infer::sched::Config as SchedConfig;
use tsr_infer::supervisor::{CandidatesLauncher, Config as SupConfig, Supervisor};

use crate::views::Core;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ModelStatus {
    /// `not_set_up`, `ready` (files present, loads on first job), `loaded`, `restarting`, `unavailable`.
    pub state: String,
    pub backend: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Status {
    pub paused: bool,
    pub capturing: bool,
    pub model: ModelStatus,
    pub queue: [usize; 3],
    pub counts: crate::views::Counts,
}

/// `%LOCALAPPDATA%\Tessara`, or `TSR_DATA_DIR` for test runs against a synthetic store
/// (`tsr-eval seed --dest DIR`), so manual checks never touch the real profile.
pub fn data_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("TSR_DATA_DIR") {
        return PathBuf::from(d);
    }
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("Tessara")
}

pub fn local_tz() -> jiff::tz::TimeZone {
    jiff::tz::TimeZone::try_system().unwrap_or(jiff::tz::TimeZone::UTC)
}

pub fn now_ms() -> i64 {
    jiff::Timestamp::now().as_millisecond()
}

struct Deferring;
impl Runner for Deferring {
    fn run(
        &mut self,
        _: &tsr_infer::sched::Dispatch,
        _: Option<&dyn tsr_infer::client::ChatModel>,
        _: &tsr_infer::cancel::CancelToken,
    ) -> tsr_infer::sched::Outcome {
        tsr_infer::sched::Outcome::Deferred { retry_in_ms: 600_000 }
    }
}

pub struct Services {
    pub views: Arc<Core>,
    pub drafts: Arc<crate::drafts::Drafts>,
    pub asks: Arc<crate::ask::Asks>,
    paused: AtomicBool,
    capture: Mutex<Option<CaptureHandle<StoreSink>>>,
    runtime: Option<Runtime<CandidatesLauncher>>,
    model_files: bool,
    stop: Arc<AtomicBool>,
    /// Writes today's brief (plan §11.2); separate from the read connection behind `views`.
    brief_conn: Mutex<rusqlite::Connection>,
}

/// Milliseconds since the user's last keyboard or mouse input.
fn idle_ms() -> u64 {
    #[cfg(windows)]
    return tsr_win::query::idle_ms();
    #[cfg(not(windows))]
    0
}

/// Rollups once a minute (plan §11.1). Without a model, code-only (extractive) summaries are
/// written directly; with a model, a P2 job is submitted and runs through the scheduler.
const ROLLUP_EVERY: Duration = Duration::from_secs(60);

fn blocklist(conn: &mut rusqlite::Connection) -> Blocklist {
    let _ = tsr_store::blocklist::sync_builtin_rules(conn, &builtin_rules());
    let rules = tsr_store::blocklist::load_rules(conn).unwrap_or_else(|_| builtin_rules());
    Blocklist::compile(&rules).0
}

/// Build launch candidates for verified model files in `models`, best backend first.
fn candidates(models: &Path, third_party: &Path) -> Option<(Vec<tsr_infer::sidecar::SidecarSpec>, Vec<String>)> {
    use tsr_infer::models::{FileStatus, MANIFEST, inspect_dir};
    let r = inspect_dir(
        models,
        &MANIFEST,
        false,
        &tsr_infer::cancel::CancelToken::new(),
        &mut |_, _| {},
    );
    let ok = |id: &str| {
        r.entries.iter().any(|e| {
            e.id == id
                && e.files
                    .iter()
                    .all(|f| matches!(f.status, FileStatus::SizeOk | FileStatus::Ok { .. }))
        })
    };
    if !ok("qwen3-vl-4b-instruct-q4_k_m") {
        return None;
    }
    let model = models.join("Qwen3VL-4B-Instruct-Q4_K_M.gguf");
    let mmproj = ok("qwen3-vl-4b-instruct-mmproj-f16").then(|| models.join("mmproj-Qwen3VL-4B-Instruct-F16.gguf"));
    let report = tsr_infer::detect(
        third_party,
        &tsr_infer::hardware::SystemAdapters,
        &tsr_infer::backend::LlamaServerProbe::default(),
    );
    let g = tsr_infer::gguf::read_gguf(&model).ok()?;
    let mm = mmproj.as_ref().and_then(|p| tsr_infer::gguf::read_gguf(p).ok());
    let needs = tsr_infer::select::ModelNeeds::from_gguf(&g, mm.as_ref(), 8192, 1.0625);
    let plan = tsr_infer::select::plan(
        &report.adapters_result(),
        &report.builds,
        &report.probes,
        &needs,
        &Default::default(),
    )
    .ok()?;
    let specs = plan
        .candidates
        .iter()
        .map(|c| tsr_infer::sidecar::SidecarSpec::from_candidate(c, model.clone(), mmproj.clone(), 8192, None))
        .collect();
    Some((specs, plan.candidates.iter().map(|c| c.label()).collect()))
}

impl Services {
    /// Start everything. `third_party` holds the pinned llama.cpp builds.
    pub fn start(dir: &Path, third_party: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(dir).map_err(|e| format!("data dir: {:?}", e.kind()))?;
        let mut writer = tsr_store::keystore::open_store(dir).map_err(|e| e.to_string())?;
        tsr_store::jobs::requeue_interrupted(&writer, now_ms()).map_err(|e| e.to_string())?;
        let bl = blocklist(&mut writer);
        let reader = tsr_store::keystore::open_store(dir).map_err(|e| e.to_string())?;
        let views = Arc::new(Core::new(reader));
        let mut draft_conn = tsr_store::keystore::open_store(dir).map_err(|e| e.to_string())?;
        let draft_bl = blocklist(&mut draft_conn);
        let mut ask_conn = tsr_store::keystore::open_store(dir).map_err(|e| e.to_string())?;
        let ask_bl = blocklist(&mut ask_conn);
        let asks = Arc::new(crate::ask::Asks::new(ask_conn, ask_bl));
        let drafts =
            Arc::new(crate::drafts::Drafts::new(draft_conn, draft_bl).map_err(|e| format!("drafts: {:?}", e.kind()))?);

        let capture = spawn_capture(move || {
            Ok(CaptureCore::new(
                WinPlatform::new()?,
                StoreSink::spawn(writer)?,
                bl,
                tsr_guard::redact::Options::default(),
            ))
        })
        .map_err(|e| format!("capture: {:?}", e.kind()))?;

        let stop = Arc::new(AtomicBool::new(false));
        let cands = candidates(&dir.join("models"), third_party);
        let model_files = cands.is_some();
        let runtime = match cands {
            None => None,
            Some((specs, labels)) => {
                let open = || {
                    let c = tsr_store::keystore::open_store(dir).map_err(|e| e.to_string())?;
                    c.busy_timeout(Duration::from_secs(10)).map_err(|e| e.to_string())?;
                    Ok::<_, String>(c)
                };
                let ex = tsr_pipeline::worker::ExtractRunner {
                    conn: open()?,
                    make_ctx: move || {
                        let tz = local_tz();
                        let bl2 = Blocklist::compile(&builtin_rules()).0;
                        tsr_pipeline::worker::CtxOwned {
                            user_name: String::new(),
                            user_aliases: Vec::new(),
                            tz: tz.iana_name().unwrap_or("UTC").to_owned(),
                            date_order: tsr_pipeline::dates::DateOrder::Dmy,
                            blocklist: bl2,
                            redact: tsr_guard::redact::Options::default(),
                            model_id: "qwen3-vl-4b-instruct-q4_k_m".into(),
                            now_ms: now_ms(),
                        }
                    },
                };
                let rollup = tsr_pipeline::worker::RollupRunner {
                    conn: open()?,
                    tz: local_tz(),
                    model_id: "qwen3-vl-4b-instruct-q4_k_m".into(),
                    redact: tsr_guard::redact::Options::default(),
                    clock: now_ms,
                    idle_ms,
                };
                let rt = Runtime::start(
                    SchedConfig::default(),
                    Supervisor::new(
                        SupConfig::default(),
                        CandidatesLauncher::new(specs, labels),
                        now_ms() as u64,
                    ),
                    IdlePolicy::default(),
                    Box::new(tsr_pipeline::worker::ByKind(vec![
                        ("extract", Box::new(ex)),
                        ("rollup", Box::new(rollup)),
                        ("draft", Box::new(drafts.runner())),
                        ("ask", Box::new(asks.runner())),
                    ])),
                    Box::new(Deferring),
                );
                // Feed durable jobs to the scheduler (extraction only: no embedder installed yet).
                let feed = open()?;
                let stop2 = stop.clone();
                let submitter = rt.submitter();
                let draft_submitter = rt.submitter();
                drafts.set_submit(Box::new(move |spec| draft_submitter.submit(spec).is_ok()));
                let ask_submitter = rt.submitter();
                asks.set_submit(Box::new(move |spec| ask_submitter.submit(spec).is_ok()));
                std::thread::Builder::new()
                    .name("tsr-job-feed".into())
                    .spawn(move || {
                        let tz = local_tz();
                        let mut last_rollup = std::time::Instant::now() - ROLLUP_EVERY;
                        while !stop2.load(Ordering::Acquire) {
                            let _ = tsr_pipeline::worker::submit_pending(&feed, &["extract"], 500, &mut |s| {
                                submitter.submit(s)
                            });
                            if last_rollup.elapsed() >= ROLLUP_EVERY {
                                last_rollup = std::time::Instant::now();
                                let now = now_ms();
                                if tsr_pipeline::rollup::due(&feed, now, &tz, idle_ms()).is_ok_and(|d| !d.is_empty()) {
                                    let _ = submitter.submit(tsr_pipeline::worker::rollup_spec(now));
                                }
                            }
                            std::thread::sleep(Duration::from_secs(5));
                        }
                    })
                    .map_err(|e| format!("feeder: {:?}", e.kind()))?;
                Some(rt)
            }
        };
        if runtime.is_none() {
            // No model: code-only rollups so Timeline week/month still has summaries.
            let mut conn = tsr_store::keystore::open_store(dir).map_err(|e| e.to_string())?;
            let _ = conn.busy_timeout(Duration::from_secs(10));
            let stop3 = stop.clone();
            std::thread::Builder::new()
                .name("tsr-rollup".into())
                .spawn(move || {
                    let tz = local_tz();
                    while !stop3.load(Ordering::Acquire) {
                        let ctx = tsr_pipeline::rollup::RollupCtx {
                            tz: &tz,
                            redact: tsr_guard::redact::Options::default(),
                            model_id: "extractive",
                            now_ms: now_ms(),
                        };
                        let cancel = tsr_infer::cancel::CancelToken::new();
                        let _ = tsr_pipeline::rollup::run_all_due(&mut conn, None, &ctx, idle_ms(), &cancel);
                        for _ in 0..ROLLUP_EVERY.as_secs() {
                            if stop3.load(Ordering::Acquire) {
                                return;
                            }
                            std::thread::sleep(Duration::from_secs(1));
                        }
                    }
                })
                .map_err(|e| format!("rollup: {:?}", e.kind()))?;
        }
        let brief_conn = tsr_store::keystore::open_store(dir).map_err(|e| e.to_string())?;
        let _ = brief_conn.busy_timeout(Duration::from_secs(10));
        Ok(Services {
            views,
            drafts,
            asks,
            paused: AtomicBool::new(true),
            capture: Mutex::new(Some(capture)),
            runtime,
            model_files,
            stop,
            brief_conn: Mutex::new(brief_conn),
        })
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Release);
        if let Some(c) = self.capture.lock().unwrap_or_else(|p| p.into_inner()).as_ref() {
            if paused { c.pause() } else { c.resume() }
        }
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Acquire)
    }

    pub fn set_on_battery(&self, on_battery: bool) {
        if let Some(rt) = &self.runtime {
            rt.set_on_battery(on_battery);
        }
    }

    pub fn status(&self) -> Status {
        let (model, queue) = match &self.runtime {
            None => (
                ModelStatus {
                    state: if self.model_files { "unavailable" } else { "not_set_up" }.into(),
                    backend: None,
                },
                [0; 3],
            ),
            Some(rt) => {
                let snap = rt.snapshot();
                let state = match snap.sidecar_state {
                    tsr_infer::supervisor::State::Ready => "loaded",
                    tsr_infer::supervisor::State::Starting | tsr_infer::supervisor::State::Backoff => "restarting",
                    tsr_infer::supervisor::State::Failed => "unavailable",
                    tsr_infer::supervisor::State::Stopped => "ready",
                };
                let backend = rt.with_supervisor(|s| s.launcher().active().map(str::to_owned));
                (
                    ModelStatus {
                        state: state.into(),
                        backend,
                    },
                    snap.queue_depth,
                )
            }
        };
        let counts = self
            .views
            .counts(now_ms(), &local_tz())
            .unwrap_or(crate::views::Counts {
                open: 0,
                due_today: 0,
                overdue: 0,
                needs_review: 0,
                items_today: 0,
            });
        Status {
            paused: self.is_paused(),
            capturing: !self.is_paused(),
            model,
            queue,
            counts,
        }
    }

    /// Flush sessions, stop capture, stop the sidecar (plan §4.3 step 4).
    /// Make today's brief if it is due (first unlock or start after 05:00 on a new day).
    /// Returns it with how long it took (ms).
    pub fn brief_if_due(&self) -> Option<(tsr_pipeline::brief::Brief, u128)> {
        let t = std::time::Instant::now();
        let c = self.brief_conn.lock().unwrap_or_else(|p| p.into_inner());
        let tz = local_tz();
        let now = now_ms();
        tsr_pipeline::brief::due_day(&c, now, &tz, tsr_pipeline::brief::DEFAULT_BRIEF_MINUTE).ok()??;
        let b = tsr_pipeline::brief::generate(&c, now, &tz).ok()?;
        Some((b, t.elapsed().as_millis()))
    }

    pub fn shutdown(&self) {
        self.stop.store(true, Ordering::Release);
        if let Some(c) = self.capture.lock().unwrap_or_else(|p| p.into_inner()).take() {
            let core = c.shutdown();
            let CaptureCore { sink, .. } = core;
            drop(sink.finish());
        }
    }
}
