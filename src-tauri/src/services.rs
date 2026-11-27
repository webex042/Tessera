//! Assembles the running app (plan §4.1, §4.3): encrypted store → capture thread (starts
//! paused, D12) → inference runtime (only if verified model files are present) → job workers
//! + encrypted local VectorDb + runtime hot-reload + automatic retention pruning.
//!
//! Nothing here downloads anything unprompted. Without model files the app still captures and
//! searches; extraction jobs stay queued until local GGUF or opt-in Groq BYOK is configured.
//! Without ONNX embedder files, search uses SQLCipher FTS5 cleanly.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use serde::Serialize;
use tsr_capture::engine::CaptureCore;
use tsr_capture::platform_win::{CaptureHandle, WinPlatform, spawn_capture};
use tsr_capture::sink::StoreSink;
use tsr_guard::blocklist::{Blocklist, RuleKind, RuleSpec, Scope, builtin_rules};
use tsr_infer::cancel::CancelToken;
use tsr_infer::runtime::{IdlePolicy, Runner, Runtime};
use tsr_infer::sched::{Config as SchedConfig, JobId, JobSpec, SubmitError};
use tsr_infer::supervisor::{CandidatesLauncher, Config as SupConfig, Supervisor};
use tsr_retrieval::embed::Embedder;
use tsr_store::privacy::{
    ClearReport, MAX_RETENTION_DAYS, PrivacySettings, load_privacy_settings, prune_expired, save_privacy_settings,
};
use tsr_store::vectordb::VectorDb;

use crate::setup::{
    AiSetupState, BlocklistRuleDto, ConnectivityResult, DownloadController, E_CONSENT_REQUIRED, E_FEATURE_DISABLED,
    PrivacyInfo, blocklist_dtos, hardware_summary, models_verified_flags, try_init_embedder,
};
#[cfg(feature = "groq")]
use crate::setup::{DownloadStatus, E_DISK_FULL};
use crate::views::{ApiError, Core, E_INVALID, E_NOT_FOUND, E_STORE};

type SubmitFn = Arc<dyn Fn(JobSpec) -> Result<JobId, SubmitError> + Send + Sync>;
type BgGateFn = Arc<dyn Fn() -> bool + Send + Sync>;

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
        _: &CancelToken,
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
    data_dir: PathBuf,
    third_party: PathBuf,
    shared_blocklist: Arc<RwLock<Blocklist>>,
    shared_rules: Arc<RwLock<Vec<RuleSpec>>>,
    vector_db: Arc<Mutex<VectorDb>>,
    embedder: Arc<Mutex<Option<Arc<dyn Embedder>>>>,
    paused: AtomicBool,
    capture: Mutex<Option<CaptureHandle<StoreSink>>>,
    runtime: Mutex<Option<ActiveRuntime>>,
    model_files: AtomicBool,
    #[cfg(feature = "groq")]
    quota: Mutex<Option<Arc<tsr_store::quota::QuotaTracker>>>,
    feeder_submit: Arc<Mutex<Option<SubmitFn>>>,
    feeder_allow_bg: Arc<Mutex<BgGateFn>>,
    feeder_extractive: Arc<AtomicBool>,
    feeder_embed_enabled: Arc<AtomicBool>,
    pub downloads: Arc<DownloadController>,
    stop: Arc<AtomicBool>,
    /// Writes today's brief and persists settings; separate from `views` read connection.
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
const PRUNE_CHECK_MS: i64 = 3_600_000;

fn load_compiled_blocklist(conn: &mut rusqlite::Connection) -> (Vec<RuleSpec>, Blocklist) {
    let _ = tsr_store::blocklist::sync_builtin_rules(conn, &builtin_rules());
    let rules = tsr_store::blocklist::load_rules(conn).unwrap_or_else(|_| builtin_rules());
    let compiled = Blocklist::compile(&rules).0;
    (rules, compiled)
}

/// Build launch candidates for verified model files in `models`, best backend first.
fn candidates(models: &Path, third_party: &Path) -> Option<(Vec<tsr_infer::sidecar::SidecarSpec>, Vec<String>)> {
    use tsr_infer::models::{FileStatus, MANIFEST, inspect_dir_stamped};
    let r = inspect_dir_stamped(models, &MANIFEST, &CancelToken::new());
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

struct BuiltRuntime {
    runtime: Option<ActiveRuntime>,
    model_files: bool,
    submit: Option<SubmitFn>,
    allow_bg: BgGateFn,
    extractive_when_throttled: bool,
    #[cfg(feature = "groq")]
    quota: Option<Arc<tsr_store::quota::QuotaTracker>>,
}

#[allow(clippy::too_many_arguments)]
fn build_runtime(
    dir: &Path,
    third_party: &Path,
    shared_rules: &Arc<RwLock<Vec<RuleSpec>>>,
    vector_db: &Arc<Mutex<VectorDb>>,
    embedder_slot: &Arc<Mutex<Option<Arc<dyn Embedder>>>>,
    drafts: &Arc<crate::drafts::Drafts>,
    asks: &Arc<crate::ask::Asks>,
    settings: &PrivacySettings,
) -> Result<BuiltRuntime, String> {
    let open = || {
        let c = tsr_store::keystore::open_store(dir).map_err(|e| e.to_string())?;
        c.busy_timeout(Duration::from_secs(10)).map_err(|e| e.to_string())?;
        Ok::<_, String>(c)
    };

    let active_emb = try_init_embedder(&dir.join("models"), third_party);
    *embedder_slot.lock().unwrap_or_else(|p| p.into_inner()) = active_emb.clone();

    let make_embed_runner = || -> Result<Box<dyn Runner>, String> {
        if let Some(emb) = active_emb.clone() {
            Ok(Box::new(tsr_pipeline::worker::VectorEmbedRunner {
                conn: open()?,
                vector_db: vector_db.clone(),
                embedder: emb,
                clock: now_ms,
            }))
        } else {
            Ok(Box::new(Deferring))
        }
    };

    let dotenv = load_env_fallbacks(dir);
    let provider = if env_or(&dotenv, "TSR_LLM_PROVIDER").is_some() {
        resolve_provider(&dotenv)
    } else if settings.ai_provider == "groq" && settings.cloud_consent {
        ProviderKind::Groq
    } else {
        ProviderKind::Local
    };

    type RunnerPair = (Box<dyn Runner>, Box<dyn Runner>);
    let make_runners = |model_id: String| -> Result<RunnerPair, String> {
        let m2 = model_id.clone();
        let rules_ref = shared_rules.clone();
        let ex = tsr_pipeline::worker::ExtractRunner {
            conn: open()?,
            make_ctx: move || {
                let tz = local_tz();
                let rules_guard = rules_ref.read().unwrap_or_else(|p| p.into_inner());
                let bl2 = Blocklist::compile(&rules_guard).0;
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
                        make_embed_runner()?,
                    )))
                }
            }
        }
        ProviderKind::Groq => {
            #[cfg(feature = "groq")]
            {
                let dpapi_key = tsr_store::keystore::load_groq_api_key(dir).ok().flatten();
                let env_key = env_or(&dotenv, "GROQ_API_KEY").map(zeroize::Zeroizing::new);
                match dpapi_key.or(env_key) {
                    None => {
                        eprintln!("tessara: groq provider requested without GROQ_API_KEY");
                        None
                    }
                    Some(api_key) => {
                        let mut cfg = tsr_net::GroqConfig::from_env_or(api_key);
                        if !settings.groq_model.trim().is_empty() && settings.cloud_consent {
                            cfg.model = settings.groq_model.clone();
                        } else if let Some(m) = env_or(&dotenv, "GROQ_MODEL") {
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
                                    make_embed_runner()?,
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

    let (submit, allow_bg, extractive): (Option<SubmitFn>, BgGateFn, bool) = match &runtime {
        None => {
            drafts.clear_submit();
            asks.clear_submit();
            (None, Arc::new(|| false), true)
        }
        Some(ActiveRuntime::Local(inner)) => {
            let s1 = inner.submitter();
            let s2 = inner.submitter();
            let s3 = inner.submitter();
            drafts.set_submit(Box::new(move |spec| s1.submit(spec).is_ok()));
            asks.set_submit(Box::new(move |spec| s2.submit(spec).is_ok()));
            (Some(Arc::new(move |spec| s3.submit(spec))), Arc::new(|| true), false)
        }
        #[cfg(feature = "groq")]
        Some(ActiveRuntime::External(inner)) => {
            let s1 = inner.submitter();
            let s2 = inner.submitter();
            let s3 = inner.submitter();
            drafts.set_submit(Box::new(move |spec| s1.submit(spec).is_ok()));
            asks.set_submit(Box::new(move |spec| s2.submit(spec).is_ok()));
            let q_gate = quota_tracker.clone();
            (
                Some(Arc::new(move |spec| s3.submit(spec))),
                Arc::new(move || q_gate.as_ref().is_none_or(|q| q.background_allowed(now_ms()).is_ok())),
                true,
            )
        }
    };

    Ok(BuiltRuntime {
        runtime,
        model_files,
        submit,
        allow_bg,
        extractive_when_throttled: extractive,
        #[cfg(feature = "groq")]
        quota: quota_tracker,
    })
}

impl Services {
    /// Start everything. `third_party` holds pinned `llama.cpp` builds and `onnxruntime.dll`.
    pub fn start(dir: &Path, third_party: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(dir).map_err(|e| format!("data dir: {:?}", e.kind()))?;
        let mut writer = tsr_store::keystore::open_store(dir).map_err(|e| e.to_string())?;
        tsr_store::jobs::requeue_interrupted(&writer, now_ms()).map_err(|e| e.to_string())?;

        let (rules, compiled_bl) = load_compiled_blocklist(&mut writer);
        let shared_rules = Arc::new(RwLock::new(rules));
        let shared_blocklist = Arc::new(RwLock::new(compiled_bl));

        // Open AES-256-GCM VectorDb (automatically migrates any legacy v1 with `.v1.bak`).
        let vdb = tsr_store::keystore::open_vector_db(dir).map_err(|e| e.to_string())?;
        let vector_db = Arc::new(Mutex::new(vdb));
        let embedder: Arc<Mutex<Option<Arc<dyn Embedder>>>> = Arc::new(Mutex::new(None));

        // Prune expired memories before spawning UI/readers when retention policy is set.
        let mut boot_conn = tsr_store::keystore::open_store(dir).map_err(|e| e.to_string())?;
        let _ = boot_conn.busy_timeout(Duration::from_secs(10));
        let settings = load_privacy_settings(&boot_conn).unwrap_or_default();
        if settings.retention_days > 0 {
            let cutoff = now_ms().saturating_sub(i64::from(settings.retention_days) * 86_400_000);
            let mut v = vector_db.lock().unwrap_or_else(|p| p.into_inner());
            let _ = prune_expired(&mut boot_conn, Some(&mut *v), cutoff, now_ms());
        }

        let reader = tsr_store::keystore::open_store(dir).map_err(|e| e.to_string())?;
        let views = Arc::new(
            Core::new(reader)
                .with_vector_db(vector_db.clone())
                .with_embedder(embedder.clone())
                .with_shared_blocklist(shared_blocklist.clone()),
        );
        let draft_conn = tsr_store::keystore::open_store(dir).map_err(|e| e.to_string())?;
        let ask_conn = tsr_store::keystore::open_store(dir).map_err(|e| e.to_string())?;
        let bl_initial = Blocklist::compile(&shared_rules.read().unwrap_or_else(|p| p.into_inner())).0;
        let asks = Arc::new(
            crate::ask::Asks::new(ask_conn, bl_initial)
                .with_shared_blocklist(shared_blocklist.clone())
                .with_vector_db(vector_db.clone())
                .with_embedder(embedder.clone()),
        );
        let bl_draft = Blocklist::compile(&shared_rules.read().unwrap_or_else(|p| p.into_inner())).0;
        let drafts = Arc::new(
            crate::drafts::Drafts::new(draft_conn, bl_draft)
                .map_err(|e| format!("drafts: {:?}", e.kind()))?
                .with_shared_blocklist(shared_blocklist.clone()),
        );

        let cap_bl = shared_blocklist.clone();
        let capture = spawn_capture(move || {
            let init_bl = Blocklist::compile(&builtin_rules()).0;
            Ok(CaptureCore::new(
                WinPlatform::new()?,
                StoreSink::spawn(writer)?,
                init_bl,
                tsr_guard::redact::Options::default(),
            )
            .with_shared_blocklist(cap_bl))
        })
        .map_err(|e| format!("capture: {:?}", e.kind()))?;

        let stop = Arc::new(AtomicBool::new(false));
        let built = build_runtime(
            dir,
            third_party,
            &shared_rules,
            &vector_db,
            &embedder,
            &drafts,
            &asks,
            &settings,
        )?;

        let feeder_submit = Arc::new(Mutex::new(built.submit));
        let feeder_allow_bg = Arc::new(Mutex::new(built.allow_bg));
        let feeder_extractive = Arc::new(AtomicBool::new(built.extractive_when_throttled));
        let feeder_embed_enabled = Arc::new(AtomicBool::new(
            embedder.lock().unwrap_or_else(|p| p.into_inner()).is_some(),
        ));

        let feed = tsr_store::keystore::open_store(dir).map_err(|e| e.to_string())?;
        let _ = feed.busy_timeout(Duration::from_secs(10));
        spawn_feeder(
            feed,
            stop.clone(),
            feeder_submit.clone(),
            feeder_allow_bg.clone(),
            feeder_extractive.clone(),
            feeder_embed_enabled.clone(),
            vector_db.clone(),
            embedder.clone(),
        )?;

        Ok(Services {
            views,
            drafts,
            asks,
            data_dir: dir.to_path_buf(),
            third_party: third_party.to_path_buf(),
            shared_blocklist,
            shared_rules,
            vector_db,
            embedder,
            paused: AtomicBool::new(true),
            capture: Mutex::new(Some(capture)),
            runtime: Mutex::new(built.runtime),
            model_files: AtomicBool::new(built.model_files),
            #[cfg(feature = "groq")]
            quota: Mutex::new(built.quota),
            feeder_submit,
            feeder_allow_bg,
            feeder_extractive,
            feeder_embed_enabled,
            downloads: Arc::new(DownloadController::default()),
            stop,
            brief_conn: Mutex::new(boot_conn),
        })
    }

    pub fn reload_runtime(&self) -> Result<(), String> {
        let settings = {
            let c = self.brief_conn.lock().unwrap_or_else(|p| p.into_inner());
            load_privacy_settings(&c).unwrap_or_default()
        };
        // Shut down existing runtime first so any llama-server process exits cleanly before replacement.
        {
            self.drafts.clear_submit();
            self.asks.clear_submit();
            *self.feeder_submit.lock().unwrap_or_else(|p| p.into_inner()) = None;
            let old = self.runtime.lock().unwrap_or_else(|p| p.into_inner()).take();
            drop(old);
        }
        let built = build_runtime(
            &self.data_dir,
            &self.third_party,
            &self.shared_rules,
            &self.vector_db,
            &self.embedder,
            &self.drafts,
            &self.asks,
            &settings,
        )?;
        *self.runtime.lock().unwrap_or_else(|p| p.into_inner()) = built.runtime;
        self.model_files.store(built.model_files, Ordering::Release);
        #[cfg(feature = "groq")]
        {
            *self.quota.lock().unwrap_or_else(|p| p.into_inner()) = built.quota;
        }
        *self.feeder_submit.lock().unwrap_or_else(|p| p.into_inner()) = built.submit;
        *self.feeder_allow_bg.lock().unwrap_or_else(|p| p.into_inner()) = built.allow_bg;
        self.feeder_extractive
            .store(built.extractive_when_throttled, Ordering::Release);
        self.feeder_embed_enabled.store(
            self.embedder.lock().unwrap_or_else(|p| p.into_inner()).is_some(),
            Ordering::Release,
        );
        Ok(())
    }

    pub fn reload_blocklist(&self) -> Result<(), ApiError> {
        let rules = {
            let mut c = self.brief_conn.lock().unwrap_or_else(|p| p.into_inner());
            load_compiled_blocklist(&mut c)
        };
        *self.shared_rules.write().unwrap_or_else(|p| p.into_inner()) = rules.0;
        *self.shared_blocklist.write().unwrap_or_else(|p| p.into_inner()) = rules.1;
        Ok(())
    }

    pub fn privacy_info(&self) -> Result<PrivacyInfo, ApiError> {
        let c = self.brief_conn.lock().unwrap_or_else(|p| p.into_inner());
        let s = load_privacy_settings(&c)?;
        let (has_llm, _, _) = models_verified_flags(&self.data_dir.join("models"));
        let vec_count = self.vector_db.lock().unwrap_or_else(|p| p.into_inner()).len();
        Ok(PrivacyInfo {
            retention_days: s.retention_days,
            ai_provider: s.ai_provider,
            groq_model: s.groq_model,
            cloud_consent: s.cloud_consent,
            has_groq_key: tsr_store::keystore::has_groq_api_key(&self.data_dir),
            has_local_model: has_llm,
            vector_records: vec_count,
            last_pruned_at: s.last_pruned_at,
        })
    }

    pub fn set_retention_days(&self, days: u32) -> Result<PrivacyInfo, ApiError> {
        if days > MAX_RETENTION_DAYS {
            return Err(E_INVALID);
        }
        {
            let mut c = self.brief_conn.lock().unwrap_or_else(|p| p.into_inner());
            let mut s = load_privacy_settings(&c)?;
            s.retention_days = days;
            save_privacy_settings(&c, &s)?;
            if days > 0 {
                let cutoff = now_ms().saturating_sub(i64::from(days) * 86_400_000);
                let mut v = self.vector_db.lock().unwrap_or_else(|p| p.into_inner());
                let _ = prune_expired(&mut c, Some(&mut *v), cutoff, now_ms())?;
            }
        }
        self.privacy_info()
    }

    pub fn list_blocklist_rules(&self) -> Result<Vec<BlocklistRuleDto>, ApiError> {
        let c = self.brief_conn.lock().unwrap_or_else(|p| p.into_inner());
        blocklist_dtos(&c)
    }

    pub fn set_blocklist_rule_enabled(&self, key: &str, enabled: bool) -> Result<Vec<BlocklistRuleDto>, ApiError> {
        if key.trim().is_empty() || key.len() > 120 {
            return Err(E_INVALID);
        }
        let updated = {
            let c = self.brief_conn.lock().unwrap_or_else(|p| p.into_inner());
            tsr_store::blocklist::set_enabled(&c, key.trim(), enabled)?
        };
        if !updated {
            return Err(E_NOT_FOUND);
        }
        self.reload_blocklist()?;
        self.list_blocklist_rules()
    }

    pub fn add_blocklist_rule(
        &self,
        kind: &str,
        scope: &str,
        pattern: &str,
        is_regex: bool,
    ) -> Result<Vec<BlocklistRuleDto>, ApiError> {
        let k = RuleKind::parse(kind).ok_or(E_INVALID)?;
        let sc = Scope::parse(scope).ok_or(E_INVALID)?;
        {
            let mut c = self.brief_conn.lock().unwrap_or_else(|p| p.into_inner());
            tsr_store::blocklist::add_user_rule(&mut c, k, sc, pattern, is_regex).map_err(|e| match e {
                tsr_store::StoreError::InvalidRule => E_INVALID,
                _ => E_STORE,
            })?;
        }
        self.reload_blocklist()?;
        self.list_blocklist_rules()
    }

    pub fn remove_blocklist_rule(&self, key: &str) -> Result<Vec<BlocklistRuleDto>, ApiError> {
        let removed = {
            let c = self.brief_conn.lock().unwrap_or_else(|p| p.into_inner());
            tsr_store::blocklist::remove_user_rule(&c, key.trim())?
        };
        if !removed {
            return Err(E_NOT_FOUND);
        }
        self.reload_blocklist()?;
        self.list_blocklist_rules()
    }

    pub fn ai_setup_state(&self) -> Result<AiSetupState, ApiError> {
        let s = {
            let c = self.brief_conn.lock().unwrap_or_else(|p| p.into_inner());
            load_privacy_settings(&c)?
        };
        let dotenv = load_env_fallbacks(&self.data_dir);
        let dpapi_key = tsr_store::keystore::has_groq_api_key(&self.data_dir);
        let env_key = env_or(&dotenv, "GROQ_API_KEY").is_some();
        let (has_groq_key, groq_key_source) = if dpapi_key {
            (true, "dpapi".to_owned())
        } else if env_key {
            (true, "env".to_owned())
        } else {
            (false, "none".to_owned())
        };
        let (has_llm, _, has_emb_files) = models_verified_flags(&self.data_dir.join("models"));
        let has_embedder = self.embedder.lock().unwrap_or_else(|p| p.into_inner()).is_some() || has_emb_files;
        let st = self.status();
        let active_provider = if env_or(&dotenv, "TSR_LLM_PROVIDER").is_some() {
            match resolve_provider(&dotenv) {
                ProviderKind::Groq => "groq".to_owned(),
                ProviderKind::Local => "local".to_owned(),
            }
        } else {
            s.ai_provider
        };
        Ok(AiSetupState {
            provider: active_provider,
            runtime_state: st.model.state,
            backend: st.model.backend,
            cloud_consent: s.cloud_consent,
            groq_model: s.groq_model,
            has_groq_key,
            groq_key_source,
            has_local_model: has_llm,
            has_embedder,
            groq_feature_supported: cfg!(feature = "groq"),
            hardware: hardware_summary(&self.data_dir, &self.third_party),
            download: self.downloads.snapshot(),
        })
    }

    pub fn set_ai_provider(
        &self,
        provider: &str,
        groq_model: Option<String>,
        cloud_consent: Option<bool>,
    ) -> Result<AiSetupState, ApiError> {
        if !matches!(provider, "local" | "groq") {
            return Err(E_INVALID);
        }
        {
            let c = self.brief_conn.lock().unwrap_or_else(|p| p.into_inner());
            let mut s = load_privacy_settings(&c)?;
            if let Some(m) = groq_model {
                s.groq_model = m.trim().to_owned();
            }
            if let Some(consent) = cloud_consent {
                s.cloud_consent = consent;
            }
            if provider == "groq" && !s.cloud_consent {
                return Err(E_CONSENT_REQUIRED);
            }
            s.ai_provider = provider.to_owned();
            save_privacy_settings(&c, &s).map_err(|_| E_INVALID)?;
        }
        self.reload_runtime().map_err(|_| E_STORE)?;
        self.ai_setup_state()
    }

    pub fn save_groq_config(
        &self,
        api_key: Option<String>,
        model: &str,
        cloud_consent: bool,
    ) -> Result<AiSetupState, ApiError> {
        if !cfg!(feature = "groq") {
            return Err(E_FEATURE_DISABLED);
        }
        if !cloud_consent {
            return Err(E_CONSENT_REQUIRED);
        }
        if let Some(k) = api_key {
            let trimmed = k.trim();
            if !trimmed.is_empty() {
                tsr_store::keystore::save_groq_api_key(&self.data_dir, trimmed).map_err(|_| E_STORE)?;
            }
        }
        {
            let c = self.brief_conn.lock().unwrap_or_else(|p| p.into_inner());
            let mut s = load_privacy_settings(&c)?;
            s.groq_model = model.trim().to_owned();
            s.cloud_consent = true;
            s.ai_provider = "groq".to_owned();
            save_privacy_settings(&c, &s).map_err(|_| E_INVALID)?;
        }
        self.reload_runtime().map_err(|_| E_STORE)?;
        self.ai_setup_state()
    }

    pub fn remove_groq_config(&self) -> Result<AiSetupState, ApiError> {
        tsr_store::keystore::remove_groq_api_key(&self.data_dir).map_err(|_| E_STORE)?;
        {
            let c = self.brief_conn.lock().unwrap_or_else(|p| p.into_inner());
            let mut s = load_privacy_settings(&c)?;
            if s.ai_provider == "groq" {
                s.ai_provider = "local".to_owned();
            }
            s.cloud_consent = false;
            save_privacy_settings(&c, &s)?;
        }
        self.reload_runtime().map_err(|_| E_STORE)?;
        self.ai_setup_state()
    }

    pub fn test_groq_connection(
        &self,
        api_key: Option<String>,
        model: Option<String>,
    ) -> Result<ConnectivityResult, ApiError> {
        #[cfg(feature = "groq")]
        {
            let dotenv = load_env_fallbacks(&self.data_dir);
            let s = {
                let c = self.brief_conn.lock().unwrap_or_else(|p| p.into_inner());
                load_privacy_settings(&c).unwrap_or_default()
            };
            let explicit = api_key
                .map(|k| k.trim().to_owned())
                .filter(|k| !k.is_empty())
                .map(zeroize::Zeroizing::new);
            let key = explicit
                .or_else(|| tsr_store::keystore::load_groq_api_key(&self.data_dir).ok().flatten())
                .or_else(|| env_or(&dotenv, "GROQ_API_KEY").map(zeroize::Zeroizing::new));
            let Some(api_key) = key else {
                return Ok(ConnectivityResult {
                    ok: false,
                    latency_ms: 0,
                    code: Some("missing_key".into()),
                });
            };
            let target_model = model
                .map(|m| m.trim().to_owned())
                .filter(|m| !m.is_empty())
                .unwrap_or(s.groq_model);
            let ep = env_or(&dotenv, "TSR_GROQ_BASE_URL");
            Ok(crate::setup::run_groq_connectivity_probe(api_key, target_model, ep))
        }
        #[cfg(not(feature = "groq"))]
        {
            let _ = (api_key, model);
            Err(E_FEATURE_DISABLED)
        }
    }

    pub fn start_model_download(
        self: &Arc<Self>,
        target: &str,
        approved: bool,
        app: tauri::AppHandle,
    ) -> Result<AiSetupState, ApiError> {
        if !approved {
            return Err(E_CONSENT_REQUIRED);
        }
        if !matches!(target, "local_llm" | "embedder") {
            return Err(E_INVALID);
        }
        #[cfg(not(feature = "groq"))]
        {
            let _ = app;
            Err(E_FEATURE_DISABLED)
        }
        #[cfg(feature = "groq")]
        {
            use tauri::Emitter;
            use tsr_infer::models::MANIFEST;
            use tsr_net::download::{Approval, DownloadConfig, download_model};

            let ids: Vec<&'static str> = if target == "local_llm" {
                vec!["qwen3-vl-4b-instruct-q4_k_m", "qwen3-vl-4b-instruct-mmproj-f16"]
            } else {
                vec![crate::setup::EMBEDDER_MODEL_ID]
            };
            let entries: Vec<_> = MANIFEST
                .iter()
                .filter(|m| ids.contains(&m.id.as_str()))
                .cloned()
                .collect();
            let total_bytes: u64 = entries.iter().map(|m| m.total_size()).sum();

            if let Some(free) = crate::setup::free_disk_bytes(&self.data_dir)
                && free < total_bytes.saturating_add(256 << 20)
            {
                return Err(E_DISK_FULL);
            }

            {
                let mut st = self.downloads.status.lock().unwrap_or_else(|p| p.into_inner());
                if st.active {
                    return self.ai_setup_state();
                }
                *st = DownloadStatus {
                    active: true,
                    target: Some(target.to_owned()),
                    phase: "starting".to_owned(),
                    done_bytes: 0,
                    total_bytes,
                    file: None,
                    error: None,
                };
            }

            let cancel = CancelToken::new();
            *self.downloads.cancel.lock().unwrap_or_else(|p| p.into_inner()) = Some(cancel.clone());

            let this = self.clone();
            let target_owned = target.to_owned();
            std::thread::Builder::new()
                .name("tsr-model-download".into())
                .spawn(move || {
                    let models_dir = this.data_dir.join("models");
                    let approval = Approval::by_user(&ids, now_ms());
                    let cfg = DownloadConfig::new(models_dir);
                    let mut completed_prefix = 0u64;
                    let mut last_file = String::new();
                    let mut current_file_total = 0u64;
                    let mut final_err: Option<String> = None;

                    for entry in &entries {
                        if let Err(err) = download_model(entry, &approval, &cfg, &cancel, &mut |p| {
                            if last_file != p.file {
                                if !last_file.is_empty() {
                                    completed_prefix = completed_prefix.saturating_add(current_file_total);
                                }
                                last_file = p.file.to_owned();
                                current_file_total = p.total;
                            }
                            let overall_done = completed_prefix.saturating_add(p.done).min(total_bytes);
                            let snap = DownloadStatus {
                                active: true,
                                target: Some(target_owned.clone()),
                                phase: match p.phase {
                                    tsr_net::download::Phase::Downloading => "downloading".into(),
                                    tsr_net::download::Phase::Verifying => "verifying".into(),
                                },
                                done_bytes: overall_done,
                                total_bytes,
                                file: Some(p.file.to_owned()),
                                error: None,
                            };
                            *this.downloads.status.lock().unwrap_or_else(|e| e.into_inner()) = snap.clone();
                            let _ = app.emit("model_download_progress", &snap);
                        }) {
                            final_err = Some(err.to_string());
                            break;
                        }
                    }

                    let done_snap = {
                        let mut st = this.downloads.status.lock().unwrap_or_else(|e| e.into_inner());
                        st.active = false;
                        st.phase = if final_err.is_none() {
                            "complete".into()
                        } else if cancel.is_cancelled() {
                            "cancelled".into()
                        } else {
                            "failed".into()
                        };
                        if final_err.is_none() {
                            st.done_bytes = total_bytes;
                        }
                        st.error = final_err;
                        st.clone()
                    };
                    *this.downloads.cancel.lock().unwrap_or_else(|e| e.into_inner()) = None;
                    if done_snap.phase == "complete" {
                        let _ = this.reload_runtime();
                        let _ = app.emit("status_changed", this.status());
                    }
                    let _ = app.emit("model_download_progress", &done_snap);
                })
                .map_err(|_| E_STORE)?;

            self.ai_setup_state()
        }
    }

    pub fn cancel_model_download(&self) -> Result<AiSetupState, ApiError> {
        self.downloads.cancel();
        self.ai_setup_state()
    }

    pub fn delete_item(&self, item_id: i64) -> Result<(), ApiError> {
        self.views.delete_item(item_id, now_ms())
    }

    pub fn clear_memories(&self, token: &str) -> Result<ClearReport, ApiError> {
        self.views.clear_memories(token, std::time::Instant::now(), now_ms())
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
        if let Some(rt) = self.runtime.lock().unwrap_or_else(|p| p.into_inner()).as_ref() {
            rt.set_on_battery(on_battery);
        }
    }

    pub fn status(&self) -> Status {
        let rt_guard = self.runtime.lock().unwrap_or_else(|p| p.into_inner());
        let has_files = self.model_files.load(Ordering::Acquire);
        let (model, queue) = match rt_guard.as_ref() {
            None => (
                ModelStatus {
                    state: if has_files { "unavailable" } else { "not_set_up" }.into(),
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
                if let Some(q) = self.quota.lock().unwrap_or_else(|p| p.into_inner()).as_ref()
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
        drop(rt_guard);
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

    /// Content-free diagnostics bundle (Stage 9); nothing is sent anywhere.
    pub fn diagnostics(&self, start_error: Option<&str>) -> serde_json::Value {
        let c = self.brief_conn.lock().unwrap_or_else(|p| p.into_inner());
        let rt = self
            .runtime
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
            .and_then(|r| serde_json::to_value(r.snapshot()).ok());
        crate::diagnostics::bundle(&c, &self.status(), rt, start_error)
    }

    /// Make today's brief if it is due (first unlock or start after 05:00 on a new day).
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
        self.downloads.cancel();
        if let Some(c) = self.capture.lock().unwrap_or_else(|p| p.into_inner()).take() {
            let core = c.shutdown();
            let CaptureCore { sink, .. } = core;
            drop(sink.finish());
        }
        let _ = self.runtime.lock().unwrap_or_else(|p| p.into_inner()).take();
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_feeder(
    mut feed: rusqlite::Connection,
    stop: Arc<AtomicBool>,
    submit_slot: Arc<Mutex<Option<SubmitFn>>>,
    allow_bg_slot: Arc<Mutex<BgGateFn>>,
    extractive_flag: Arc<AtomicBool>,
    embed_enabled: Arc<AtomicBool>,
    vector_db: Arc<Mutex<VectorDb>>,
    embedder_slot: Arc<Mutex<Option<Arc<dyn Embedder>>>>,
) -> Result<(), String> {
    std::thread::Builder::new()
        .name("tsr-job-feed".into())
        .spawn(move || {
            let tz = local_tz();
            let mut last_rollup = std::time::Instant::now() - ROLLUP_EVERY;
            let mut last_prune = now_ms();
            while !stop.load(Ordering::Acquire) {
                let now = now_ms();
                if now - last_prune >= PRUNE_CHECK_MS {
                    last_prune = now;
                    if let Ok(settings) = load_privacy_settings(&feed)
                        && settings.retention_days > 0
                    {
                        let cutoff = now.saturating_sub(i64::from(settings.retention_days) * 86_400_000);
                        let mut v = vector_db.lock().unwrap_or_else(|p| p.into_inner());
                        let _ = prune_expired(&mut feed, Some(&mut *v), cutoff, now);
                    }
                }

                let submit_opt = submit_slot.lock().unwrap_or_else(|p| p.into_inner()).clone();
                if let Some(submit) = submit_opt {
                    let allow_bg = allow_bg_slot.lock().unwrap_or_else(|p| p.into_inner()).clone();
                    let bg_ok = allow_bg();
                    if bg_ok {
                        let batch = if extractive_flag.load(Ordering::Acquire) {
                            5
                        } else {
                            500
                        };
                        let _ = tsr_pipeline::worker::submit_pending(&feed, &["extract"], batch, &mut |s| submit(s));
                        if embed_enabled.load(Ordering::Acquire) {
                            let _ = tsr_pipeline::worker::submit_pending(&feed, &["embed"], 100, &mut |s| submit(s));
                        }
                    }
                    if last_rollup.elapsed() >= ROLLUP_EVERY {
                        last_rollup = std::time::Instant::now();
                        if tsr_pipeline::rollup::due(&feed, now, &tz, idle_ms()).is_ok_and(|d| !d.is_empty()) {
                            if bg_ok {
                                let _ = submit(tsr_pipeline::worker::rollup_spec(now));
                            } else if extractive_flag.load(Ordering::Acquire) {
                                let ctx = tsr_pipeline::rollup::RollupCtx {
                                    tz: &tz,
                                    redact: tsr_guard::redact::Options::default(),
                                    model_id: "extractive",
                                    now_ms: now,
                                };
                                let _ = tsr_pipeline::rollup::run_all_due(
                                    &mut feed,
                                    None,
                                    &ctx,
                                    idle_ms(),
                                    &CancelToken::new(),
                                );
                            }
                        }
                    }
                } else {
                    // Even without an LLM runtime, embed new items into encrypted VectorDb if embedder exists
                    // and keep code-only extractive rollups updated for Timeline week/month.
                    if let Some(emb) = embedder_slot.lock().unwrap_or_else(|p| p.into_inner()).clone()
                        && let Ok(queued) = tsr_store::jobs::queued(&feed, "embed", 8)
                    {
                        for (_, item_id) in queued {
                            if stop.load(Ordering::Acquire) {
                                break;
                            }
                            let mut v = vector_db.lock().unwrap_or_else(|p| p.into_inner());
                            let _ = tsr_retrieval::ingest::embed_item_to_vectordb(
                                &mut feed,
                                &mut v,
                                item_id,
                                &*emb,
                                None,
                                now_ms(),
                            );
                        }
                    }
                    if last_rollup.elapsed() >= ROLLUP_EVERY {
                        last_rollup = std::time::Instant::now();
                        let ctx = tsr_pipeline::rollup::RollupCtx {
                            tz: &tz,
                            redact: tsr_guard::redact::Options::default(),
                            model_id: "extractive",
                            now_ms: now,
                        };
                        let _ =
                            tsr_pipeline::rollup::run_all_due(&mut feed, None, &ctx, idle_ms(), &CancelToken::new());
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
