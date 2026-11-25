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

enum ActiveRuntime {
    Local(Runtime<CandidatesLauncher>),
    #[cfg(feature = "groq")]
    External(Runtime<tsr_infer::runtime::NoLaunch>),
}

impl ActiveRuntime {
    fn set_on_battery(&self, on_battery: bool) {
        match self {
            ActiveRuntime::Local(rt) => rt.set_on_battery(on_battery),
            #[cfg(feature = "groq")]
            ActiveRuntime::External(rt) => rt.set_on_battery(on_battery),
        }
    }

    fn snapshot(&self) -> tsr_infer::metrics::Snapshot {
        match self {
            ActiveRuntime::Local(rt) => rt.snapshot(),
            #[cfg(feature = "groq")]
            ActiveRuntime::External(rt) => rt.snapshot(),
        }
    }

    fn backend_label(&self) -> Option<String> {
        match self {
            ActiveRuntime::Local(rt) => rt
                .try_with_supervisor(|s| s.launcher().active().map(str::to_owned))
                .flatten(),
            #[cfg(feature = "groq")]
            ActiveRuntime::External(rt) => rt.external_label().map(str::to_owned),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ProviderKind {
    Local,
    Groq,
}

fn parse_dotenv(path: &Path, out: &mut std::collections::HashMap<String, String>) {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return;
    };
    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        let t = t.strip_prefix("export ").unwrap_or(t).trim_start();
        let Some((k, v)) = t.split_once('=') else {
            continue;
        };
        let key = k.trim();
        let mut val = v.trim();
        if ((val.starts_with('"') && val.ends_with('"')) || (val.starts_with('\'') && val.ends_with('\'')))
            && val.len() >= 2
        {
            val = &val[1..val.len() - 1];
        }
        if !key.is_empty() && !val.is_empty() {
            out.entry(key.to_owned()).or_insert_with(|| val.to_owned());
        }
    }
}

fn load_env_fallbacks(data_dir: &Path) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    if let Ok(cwd) = std::env::current_dir() {
        parse_dotenv(&cwd.join(".env.local"), &mut map);
        parse_dotenv(&cwd.join(".env"), &mut map);
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        parse_dotenv(&dir.join(".env"), &mut map);
    }
    parse_dotenv(&data_dir.join(".env"), &mut map);
    map
}

fn env_or(dotenv: &std::collections::HashMap<String, String>, key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .or_else(|| dotenv.get(key).cloned())
}

fn resolve_provider(dotenv: &std::collections::HashMap<String, String>) -> ProviderKind {
    match env_or(dotenv, "TSR_LLM_PROVIDER")
        .unwrap_or_else(|| "local".to_owned())
        .to_ascii_lowercase()
        .as_str()
    {
        "groq" => ProviderKind::Groq,
        _ => ProviderKind::Local,
    }
}

pub struct Services {
    pub views: Arc<Core>,
    pub drafts: Arc<crate::drafts::Drafts>,
    pub asks: Arc<crate::ask::Asks>,
    paused: AtomicBool,
    capture: Mutex<Option<CaptureHandle<StoreSink>>>,
    runtime: Option<ActiveRuntime>,
    model_files: bool,
    #[cfg(feature = "groq")]
    quota: Option<Arc<tsr_store::quota::QuotaTracker>>,
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
    use tsr_infer::models::{FileStatus, MANIFEST, inspect_dir_stamped};
    // Digests against tools/models.lock: fully on first use or after any change, then trusted
    // through the stamp while size and modification time are unchanged (Stage 7, ADR-0009).
    let r = inspect_dir_stamped(models, &MANIFEST, &tsr_infer::cancel::CancelToken::new());
    let ok = |id: &str| {
        r.entries.iter().any(|e| {
            e.id == id
                && e.files
                    .iter()
                    .all(|f| matches!(f.status, FileStatus::Ok { digest_checked: true }))
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
        let dotenv = load_env_fallbacks(dir);
        let provider = resolve_provider(&dotenv);
        let open = || {
            let c = tsr_store::keystore::open_store(dir).map_err(|e| e.to_string())?;
            c.busy_timeout(Duration::from_secs(10)).map_err(|e| e.to_string())?;
            Ok::<_, String>(c)
        };
        type RunnerPair = (Box<dyn Runner>, Box<dyn Runner>);
        let make_runners = |model_id: String| -> Result<RunnerPair, String> {
            let m2 = model_id.clone();
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
                        model_id: model_id.clone(),
                        now_ms: now_ms(),
                    }
                },
            };
            let rollup = tsr_pipeline::worker::RollupRunner {
                conn: open()?,
                tz: local_tz(),
                model_id: m2,
                redact: tsr_guard::redact::Options::default(),
                clock: now_ms,
                idle_ms,
            };
            Ok((Box::new(ex), Box::new(rollup)))
        };

        let mut model_files = false;
        #[cfg(feature = "groq")]
        let mut quota_tracker: Option<Arc<tsr_store::quota::QuotaTracker>> = None;

        let runtime: Option<ActiveRuntime> = match provider {
            ProviderKind::Local => {
                let cands = candidates(&dir.join("models"), third_party);
                model_files = cands.is_some();
                match cands {
                    None => None,
                    Some((specs, labels)) => {
                        let (ex, rollup) = make_runners("qwen3-vl-4b-instruct-q4_k_m".into())?;
                        Some(ActiveRuntime::Local(Runtime::start(
                            SchedConfig::default(),
                            Supervisor::new(
                                SupConfig::default(),
                                CandidatesLauncher::new(specs, labels),
                                now_ms() as u64,
                            ),
                            IdlePolicy::default(),
                            Box::new(tsr_pipeline::worker::ByKind(vec![
                                ("extract", ex),
                                ("rollup", rollup),
                                ("draft", Box::new(drafts.runner())),
                                ("ask", Box::new(asks.runner())),
                            ])),
                            Box::new(Deferring),
                        )))
                    }
                }
            }
            ProviderKind::Groq => {
                #[cfg(feature = "groq")]
                {
                    let env_key = env_or(&dotenv, "GROQ_API_KEY").map(zeroize::Zeroizing::new);
                    let dpapi_key = tsr_store::keystore::load_groq_api_key(dir).ok().flatten();
                    match env_key.or(dpapi_key) {
                        None => {
                            eprintln!("tessara: groq provider requested without GROQ_API_KEY");
                            None
                        }
                        Some(api_key) => {
                            let mut cfg = tsr_net::GroqConfig::from_env_or(api_key);
                            if let Some(m) = env_or(&dotenv, "GROQ_MODEL") {
                                cfg.model = m;
                            }
                            if let Some(b) = env_or(&dotenv, "TSR_GROQ_BASE_URL") {
                                let b = b.trim_end_matches('/');
                                cfg.endpoint = if b.ends_with("/chat/completions") {
                                    b.to_owned()
                                } else {
                                    format!("{b}/chat/completions")
                                };
                            }
                            if let Some(e) = env_or(&dotenv, "GROQ_REASONING_EFFORT")
                                && matches!(e.to_ascii_lowercase().as_str(), "low" | "medium" | "high")
                            {
                                cfg.reasoning_effort = e.to_ascii_lowercase();
                            }
                            let model_name = cfg.model.clone();
                            let quota = Arc::new(tsr_store::quota::QuotaTracker::open(
                                open()?,
                                tsr_store::quota::QuotaLimits::default(),
                            ));
                            match tsr_net::GroqClient::new(cfg, quota.clone()) {
                                Ok(client) => {
                                    model_files = true;
                                    quota_tracker = Some(quota);
                                    let (ex, rollup) = make_runners(model_name)?;
                                    Some(ActiveRuntime::External(Runtime::start_external(
                                        SchedConfig::default(),
                                        Arc::new(client),
                                        "groq",
                                        Box::new(tsr_pipeline::worker::ByKind(vec![
                                            ("extract", ex),
                                            ("rollup", rollup),
                                            ("draft", Box::new(drafts.runner())),
                                            ("ask", Box::new(asks.runner())),
                                        ])),
                                        Box::new(Deferring),
                                    )))
                                }
                                Err(_) => {
                                    eprintln!("tessara: failed to initialize Groq client");
                                    None
                                }
                            }
                        }
                    }
                }
                #[cfg(not(feature = "groq"))]
                {
                    eprintln!("tessara: groq provider requested in build without groq feature");
                    None
                }
            }
        };

        if let Some(rt) = &runtime {
            // Wire interactive P0 submitters and start durable background feeder.
            let feed = open()?;
            let stop2 = stop.clone();
            match rt {
                ActiveRuntime::Local(inner) => {
                    let s1 = inner.submitter();
                    let s2 = inner.submitter();
                    let s3 = inner.submitter();
                    drafts.set_submit(Box::new(move |spec| s1.submit(spec).is_ok()));
                    asks.set_submit(Box::new(move |spec| s2.submit(spec).is_ok()));
                    spawn_feeder(feed, stop2, 500, move |spec| s3.submit(spec), || true, false)?;
                }
                #[cfg(feature = "groq")]
                ActiveRuntime::External(inner) => {
                    let s1 = inner.submitter();
                    let s2 = inner.submitter();
                    let s3 = inner.submitter();
                    drafts.set_submit(Box::new(move |spec| s1.submit(spec).is_ok()));
                    asks.set_submit(Box::new(move |spec| s2.submit(spec).is_ok()));
                    let q_gate = quota_tracker.clone();
                    spawn_feeder(
                        feed,
                        stop2,
                        5,
                        move |spec| s3.submit(spec),
                        move || q_gate.as_ref().is_none_or(|q| q.background_allowed(now_ms()).is_ok()),
                        true,
                    )?;
                }
            }
        }
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
            #[cfg(feature = "groq")]
            quota: quota_tracker,
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
                #[cfg_attr(not(feature = "groq"), allow(unused_mut))]
                let mut state = match snap.sidecar_state {
                    tsr_infer::supervisor::State::Ready => "loaded",
                    tsr_infer::supervisor::State::Starting | tsr_infer::supervisor::State::Backoff => "restarting",
                    tsr_infer::supervisor::State::Failed => "unavailable",
                    tsr_infer::supervisor::State::Stopped => "ready",
                };
                #[cfg(feature = "groq")]
                if let Some(q) = &self.quota
                    && q.snapshot(now_ms()).daily_exhausted
                {
                    state = "unavailable";
                }
                let backend = rt.backend_label();
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
    /// Content-free diagnostics bundle (Stage 9); nothing is sent anywhere.
    pub fn diagnostics(&self, start_error: Option<&str>) -> serde_json::Value {
        let c = self.brief_conn.lock().unwrap_or_else(|p| p.into_inner());
        let rt = self
            .runtime
            .as_ref()
            .and_then(|r| serde_json::to_value(r.snapshot()).ok());
        crate::diagnostics::bundle(&c, &self.status(), rt, start_error)
    }

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

fn spawn_feeder<F, G>(
    mut feed: rusqlite::Connection,
    stop: Arc<AtomicBool>,
    extract_batch: u32,
    submit: F,
    allow_background: G,
    extractive_fallback_when_throttled: bool,
) -> Result<(), String>
where
    F: Fn(tsr_infer::sched::JobSpec) -> Result<tsr_infer::sched::JobId, tsr_infer::sched::SubmitError> + Send + 'static,
    G: Fn() -> bool + Send + 'static,
{
    std::thread::Builder::new()
        .name("tsr-job-feed".into())
        .spawn(move || {
            let tz = local_tz();
            let mut last_rollup = std::time::Instant::now() - ROLLUP_EVERY;
            while !stop.load(Ordering::Acquire) {
                let bg_ok = allow_background();
                if bg_ok {
                    let _ =
                        tsr_pipeline::worker::submit_pending(&feed, &["extract"], extract_batch, &mut |s| submit(s));
                }
                if last_rollup.elapsed() >= ROLLUP_EVERY {
                    last_rollup = std::time::Instant::now();
                    let now = now_ms();
                    if tsr_pipeline::rollup::due(&feed, now, &tz, idle_ms()).is_ok_and(|d| !d.is_empty()) {
                        if bg_ok {
                            let _ = submit(tsr_pipeline::worker::rollup_spec(now));
                        } else if extractive_fallback_when_throttled {
                            let ctx = tsr_pipeline::rollup::RollupCtx {
                                tz: &tz,
                                redact: tsr_guard::redact::Options::default(),
                                model_id: "extractive",
                                now_ms: now,
                            };
                            let cancel = tsr_infer::cancel::CancelToken::new();
                            let _ = tsr_pipeline::rollup::run_all_due(&mut feed, None, &ctx, idle_ms(), &cancel);
                        }
                    }
                }
                std::thread::sleep(Duration::from_secs(5));
            }
        })
        .map(|_| ())
        .map_err(|e| format!("feeder: {:?}", e.kind()))
}

#[cfg(test)]
mod provider_tests {
    use super::*;

    #[test]
    fn provider_defaults_strictly_to_local() {
        let empty = std::collections::HashMap::new();
        assert!(resolve_provider(&empty) == ProviderKind::Local);

        let mut groq = std::collections::HashMap::new();
        groq.insert("TSR_LLM_PROVIDER".into(), "groq".into());
        assert!(resolve_provider(&groq) == ProviderKind::Groq);

        let mut upper = std::collections::HashMap::new();
        upper.insert("TSR_LLM_PROVIDER".into(), "GROQ".into());
        assert!(resolve_provider(&upper) == ProviderKind::Groq);

        let mut bogus = std::collections::HashMap::new();
        bogus.insert("TSR_LLM_PROVIDER".into(), "cloud".into());
        assert!(resolve_provider(&bogus) == ProviderKind::Local);
    }
}
