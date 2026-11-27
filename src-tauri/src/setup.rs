//! End-user AI onboarding, privacy/blocklist DTOs, model downloads, and ONNX runtime init.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use serde::Serialize;
use tsr_infer::cancel::CancelToken;
use tsr_infer::models::{FileStatus, MANIFEST, inspect_dir_stamped};
use tsr_infer::onboard::{Verdict, cpu_info, estimated_needs, hardware_check, memory};
use tsr_retrieval::embed::{Embedder, OnnxEmbedder};
use tsr_retrieval::runtime::{PinnedRuntime, load_pinned};

use crate::views::ApiError;

pub const PINNED_ONNXRUNTIME_SHA256: &str = "0c42dc4e278f0968fdf9ebf1b1e54a74c657a9942e70803e453c91993451cf4c";
pub const EMBEDDER_MODEL_ID: &str = "multilingual-e5-small-onnx";
pub const EMBEDDER_DIMS: usize = 384;

pub const E_CONSENT_REQUIRED: ApiError = ApiError::new("cloud_consent_required", "error.cloud_consent_required");
pub const E_DISK_FULL: ApiError = ApiError::new("insufficient_disk_space", "error.insufficient_disk_space");
pub const E_FEATURE_DISABLED: ApiError = ApiError::new("feature_unavailable", "error.feature_unavailable");

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PrivacyInfo {
    pub retention_days: u32,
    pub ai_provider: String,
    pub groq_model: String,
    pub cloud_consent: bool,
    pub has_groq_key: bool,
    pub has_local_model: bool,
    pub vector_records: usize,
    pub last_pruned_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BlocklistRuleDto {
    pub key: String,
    pub kind: String,
    pub scope: String,
    pub pattern: String,
    pub is_regex: bool,
    pub builtin: bool,
    pub enabled: bool,
    pub blocked_count: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct HardwareSummary {
    pub verdict: String,
    pub gpu_name: Option<String>,
    pub cpu_cores: usize,
    pub ram_total_mib: Option<u64>,
    pub ram_available_mib: Option<u64>,
    pub disk_free_mib: Option<u64>,
    pub local_model_bytes: u64,
    pub embedder_bytes: u64,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct DownloadStatus {
    pub active: bool,
    pub target: Option<String>,
    pub phase: String,
    pub done_bytes: u64,
    pub total_bytes: u64,
    pub file: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AiSetupState {
    pub provider: String,
    pub runtime_state: String,
    pub backend: Option<String>,
    pub cloud_consent: bool,
    pub groq_model: String,
    pub has_groq_key: bool,
    pub groq_key_source: String,
    pub has_local_model: bool,
    pub has_embedder: bool,
    pub groq_feature_supported: bool,
    pub hardware: HardwareSummary,
    pub download: DownloadStatus,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ConnectivityResult {
    pub ok: bool,
    pub latency_ms: u64,
    pub code: Option<String>,
}

#[derive(Default)]
pub struct DownloadController {
    pub status: Mutex<DownloadStatus>,
    pub cancel: Mutex<Option<CancelToken>>,
}

impl DownloadController {
    pub fn snapshot(&self) -> DownloadStatus {
        self.status.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    pub fn cancel(&self) {
        if let Some(tok) = self.cancel.lock().unwrap_or_else(|p| p.into_inner()).as_ref() {
            tok.cancel(tsr_infer::cancel::CancelReason::Cancelled);
        }
    }
}

pub fn models_verified_flags(models_dir: &Path) -> (bool, bool, bool) {
    let report = inspect_dir_stamped(models_dir, &MANIFEST, &CancelToken::new());
    let ok = |id: &str| {
        report.entries.iter().any(|e| {
            e.id == id
                && e.files
                    .iter()
                    .all(|f| matches!(f.status, FileStatus::Ok { digest_checked: true }))
        })
    };
    (
        ok("qwen3-vl-4b-instruct-q4_k_m"),
        ok("qwen3-vl-4b-instruct-mmproj-f16"),
        ok(EMBEDDER_MODEL_ID),
    )
}

pub fn resolve_onnxruntime_dll(third_party: &Path) -> Option<PathBuf> {
    let candidates = [
        third_party.join("onnxruntime/1.31.0/win-x64/onnxruntime-win-x64-1.31.0/lib/onnxruntime.dll"),
        third_party.join("onnxruntime/lib/onnxruntime.dll"),
        third_party.join("onnxruntime.dll"),
    ];
    candidates.into_iter().find(|p| p.is_file())
}

static PINNED_ORT: OnceLock<Option<PinnedRuntime>> = OnceLock::new();

pub fn try_init_embedder(models_dir: &Path, third_party: &Path) -> Option<Arc<dyn Embedder>> {
    let (_, _, has_emb_files) = models_verified_flags(models_dir);
    if !has_emb_files {
        return None;
    }
    let dll = resolve_onnxruntime_dll(third_party)?;
    let pinned = PINNED_ORT
        .get_or_init(|| match load_pinned(&dll, PINNED_ONNXRUNTIME_SHA256) {
            Ok(rt) => Some(rt),
            Err(e) => {
                eprintln!("tessara: pinned onnxruntime load skipped: {e}");
                None
            }
        })
        .as_ref()?;
    match OnnxEmbedder::from_dir(models_dir, EMBEDDER_MODEL_ID, EMBEDDER_DIMS, pinned) {
        Ok(emb) => Some(Arc::new(emb)),
        Err(e) => {
            eprintln!("tessara: onnx embedder init skipped: {e}");
            None
        }
    }
}

#[cfg(windows)]
pub fn free_disk_bytes(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    unsafe extern "system" {
        fn GetDiskFreeSpaceExW(
            lpDirectoryName: *const u16,
            lpFreeBytesAvailableToCaller: *mut u64,
            lpTotalNumberOfBytes: *mut u64,
            lpTotalNumberOfFreeBytes: *mut u64,
        ) -> i32;
    }
    let probe = if path.exists() {
        path.to_path_buf()
    } else {
        path.parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    };
    let wide: Vec<u16> = probe.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut avail = 0u64;
    let mut total = 0u64;
    let mut free = 0u64;
    let ok = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut avail, &mut total, &mut free) };
    (ok != 0).then_some(avail)
}

#[cfg(not(windows))]
pub fn free_disk_bytes(_path: &Path) -> Option<u64> {
    None
}

pub fn hardware_summary(data_dir: &Path, third_party: &Path) -> HardwareSummary {
    let report = tsr_infer::detect(
        third_party,
        &tsr_infer::hardware::SystemAdapters,
        &tsr_infer::backend::LlamaServerProbe::default(),
    );
    let mem = memory();
    let needs = estimated_needs(&MANIFEST);
    let hc = hardware_check(&report, cpu_info(), mem, needs, true, &[], &Default::default());
    let (verdict, gpu_name) = match hc.verdict {
        Verdict::Gpu { device, .. } => ("gpu".to_owned(), Some(device)),
        Verdict::PartialGpu { device, .. } => ("partial_gpu".to_owned(), Some(device)),
        Verdict::CpuOnly => ("cpu_only".to_owned(), None),
        Verdict::NoBuild => ("no_build".to_owned(), None),
    };
    let local_model_bytes = MANIFEST
        .iter()
        .filter(|m| m.id == "qwen3-vl-4b-instruct-q4_k_m" || m.id == "qwen3-vl-4b-instruct-mmproj-f16")
        .map(|m| m.total_size())
        .sum();
    let embedder_bytes = MANIFEST
        .iter()
        .filter(|m| m.id == EMBEDDER_MODEL_ID)
        .map(|m| m.total_size())
        .sum();
    let disk_free_mib = free_disk_bytes(data_dir).map(|b| b >> 20);
    let mut warnings = hc.warnings;
    if let Some(free_mib) = disk_free_mib
        && free_mib < ((local_model_bytes >> 20) + 512u64)
    {
        warnings.push(format!(
            "Local Qwen3-VL download needs ~{} MiB free; only {free_mib} MiB available.",
            (local_model_bytes >> 20) + 512u64
        ));
    }
    HardwareSummary {
        verdict,
        gpu_name,
        cpu_cores: hc.cpu.logical_cores,
        ram_total_mib: mem.map(|m| m.total_mib),
        ram_available_mib: mem.map(|m| m.available_mib),
        disk_free_mib,
        local_model_bytes,
        embedder_bytes,
        warnings,
    }
}

pub fn blocklist_dtos(conn: &rusqlite::Connection) -> Result<Vec<BlocklistRuleDto>, ApiError> {
    let rules = tsr_store::blocklist::load_rules(conn)?;
    let counts: HashMap<String, u64> = tsr_store::blocklist::blocked_counts(conn, 0)
        .unwrap_or_default()
        .into_iter()
        .collect();
    Ok(rules
        .into_iter()
        .map(|r| BlocklistRuleDto {
            blocked_count: counts.get(&r.key).copied().unwrap_or(0),
            key: r.key,
            kind: r.kind.as_str().to_owned(),
            scope: r.scope.as_str().to_owned(),
            pattern: r.pattern,
            is_regex: r.is_regex,
            builtin: r.builtin,
            enabled: r.enabled,
        })
        .collect())
}

#[cfg(feature = "groq")]
pub fn run_groq_connectivity_probe(
    api_key: zeroize::Zeroizing<String>,
    model: String,
    endpoint: Option<String>,
) -> ConnectivityResult {
    let mut cfg = tsr_net::GroqConfig::from_env_or(api_key);
    if !model.trim().is_empty() {
        cfg.model = model.trim().to_owned();
    }
    if let Some(ep) = endpoint {
        let ep = ep.trim().trim_end_matches('/');
        if !ep.is_empty() {
            cfg.endpoint = if ep.ends_with("/chat/completions") {
                ep.to_owned()
            } else {
                format!("{ep}/chat/completions")
            };
        }
    }
    let quota = Arc::new(tsr_store::quota::QuotaTracker::in_memory(
        tsr_store::quota::QuotaLimits::default(),
    ));
    let Ok(client) = tsr_net::GroqClient::new(cfg, quota) else {
        return ConnectivityResult {
            ok: false,
            latency_ms: 0,
            code: Some("invalid_config".into()),
        };
    };
    match tsr_infer::onboard::smoke_test(&client, &CancelToken::new()) {
        Ok(latency_ms) => ConnectivityResult {
            ok: true,
            latency_ms,
            code: None,
        },
        Err(tsr_infer::onboard::SmokeError::Client(e)) => {
            use tsr_infer::client::ClientError;
            let code = match e {
                ClientError::Unauthorized => "unauthorized",
                ClientError::RateLimited { .. } | ClientError::QuotaExhausted => "rate_limited",
                ClientError::Http(_) => "network_error",
                _ => "provider_error",
            };
            ConnectivityResult {
                ok: false,
                latency_ms: 0,
                code: Some(code.into()),
            }
        }
        Err(tsr_infer::onboard::SmokeError::Empty) => ConnectivityResult {
            ok: false,
            latency_ms: 0,
            code: Some("empty_reply".into()),
        },
    }
}
