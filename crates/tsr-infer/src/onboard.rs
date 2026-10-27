//! Onboarding: the hardware check and the model-setup decision (plan §17 Stage 2 task 2).
//!
//! The report keeps four levels apart, because each one proves something different:
//!
//! | level | meaning | evidence |
//! |---|---|---|
//! | `Detected` | the OS lists the adapter | DXGI / sysfs / Apple |
//! | `Installed` | a pinned llama.cpp build for the backend is on disk and intact | full file manifest |
//! | `Verified` | the build's backend initialised and listed the device | `llama-server --list-devices` |
//! | `InferenceTested` | a real model answered on that device | [`smoke_test`] result recorded by the app |
//!
//! A machine is never rejected for having no GPU: the CPU build is always a candidate (slow mode:
//! extraction only on AC power and when idle). Nothing here downloads anything.

use std::time::{Duration, Instant};

use serde::Serialize;

use crate::HardwareReport;
use crate::backend::{Backend, BackendDevice};
use crate::cancel::CancelToken;
use crate::client::{ChatModel, ChatRequest, ClientError};
use crate::models::{ModelEntry, Role};
use crate::select::{ModelNeeds, Override, SelectionPlan, plan};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CpuInfo {
    pub logical_cores: usize,
    /// Instruction-set extensions llama.cpp's CPU backends use, as detected at run time.
    pub features: Vec<&'static str>,
}

pub fn cpu_info() -> CpuInfo {
    let mut features = Vec::new();
    #[cfg(target_arch = "x86_64")]
    {
        macro_rules! f {
            ($($name:tt),*) => {$( if std::arch::is_x86_feature_detected!($name) { features.push($name); } )*};
        }
        f!("sse4.2", "avx", "avx2", "fma", "f16c", "avx512f", "avx512vnni");
    }
    #[cfg(target_arch = "aarch64")]
    {
        macro_rules! f {
            ($($name:tt),*) => {$( if std::arch::is_aarch64_feature_detected!($name) { features.push($name); } )*};
        }
        f!("neon", "dotprod", "i8mm");
    }
    CpuInfo {
        logical_cores: std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1),
        features,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Memory {
    pub total_mib: u64,
    pub available_mib: u64,
}

pub fn memory() -> Option<Memory> {
    #[cfg(windows)]
    {
        tsr_win::query::memory().map(|m| Memory {
            total_mib: m.total_mib,
            available_mib: m.available_mib,
        })
    }
    #[cfg(target_os = "linux")]
    {
        let s = std::fs::read_to_string("/proc/meminfo").ok()?;
        let get = |k: &str| {
            s.lines()
                .find(|l| l.starts_with(k))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|v| v.parse::<u64>().ok())
                .map(|kib| kib >> 10)
        };
        Some(Memory {
            total_mib: get("MemTotal:")?,
            available_mib: get("MemAvailable:")?,
        })
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Level {
    Detected,
    Installed,
    Verified,
    InferenceTested,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct BackendStatus {
    pub backend: Backend,
    pub variant: String,
    pub level: Level,
    pub devices: Vec<BackendDevice>,
    pub note: Option<String>,
}

/// A successful [`smoke_test`], as recorded by the app (settings) after onboarding.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct InferenceRecord {
    pub variant: String,
    pub device: Option<String>,
    pub model_id: String,
    pub first_token_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum Verdict {
    /// The best candidate runs fully on a GPU device.
    Gpu { backend: Backend, device: String },
    /// Some layers on the GPU, the rest on the CPU.
    PartialGpu { backend: Backend, device: String },
    /// No usable GPU: works, slowly. Extraction runs on AC power when idle.
    CpuOnly,
    /// No candidate at all (no intact build installed).
    NoBuild,
}

#[derive(Clone, Debug, Serialize)]
pub struct HardwareCheck {
    pub os: &'static str,
    pub arch: &'static str,
    pub cpu: CpuInfo,
    pub memory: Option<Memory>,
    pub adapters_detected: usize,
    pub backends: Vec<BackendStatus>,
    /// Model requirements: from the model's own GGUF headers once present, or estimated from the
    /// manifest's file sizes before download (`needs_estimated`).
    pub needs: ModelNeeds,
    pub needs_estimated: bool,
    pub plan: Result<SelectionPlan, String>,
    pub verdict: Verdict,
    pub warnings: Vec<String>,
}

/// Estimate from manifest sizes (before the files exist). KV cache: the plan's 8 k / q8_0 figure.
pub fn estimated_needs(manifest: &[ModelEntry]) -> ModelNeeds {
    let size = |r: Role| {
        manifest
            .iter()
            .filter(|m| m.role == r)
            .map(|m| m.total_size())
            .max()
            .unwrap_or(0)
    };
    ModelNeeds {
        weights_mib: size(Role::Llm) >> 20,
        mmproj_mib: size(Role::Mmproj) >> 20,
        kv_mib: 640,
        n_layers: 36,
        tensor_types: Default::default(),
    }
}

pub fn hardware_check(
    report: &HardwareReport,
    cpu: CpuInfo,
    memory: Option<Memory>,
    needs: ModelNeeds,
    needs_estimated: bool,
    tested: &[InferenceRecord],
    ovr: &Override,
) -> HardwareCheck {
    let mut backends = Vec::new();
    for b in &report.builds {
        let probe = report.probes.iter().find(|(v, _)| *v == b.variant).map(|(_, r)| r);
        let (level, devices, note) = match (b.backend, probe) {
            (Backend::Cpu, _) => (
                Level::Verified,
                vec![],
                Some("CPU build: no device probe needed".into()),
            ),
            (_, Some(Ok(d))) if !d.is_empty() => (Level::Verified, d.clone(), None),
            (_, Some(Ok(_))) => (
                Level::Installed,
                vec![],
                Some("backend initialised but found no device".into()),
            ),
            (_, Some(Err(e))) => (
                Level::Installed,
                vec![],
                Some(format!("backend did not initialise: {e}")),
            ),
            (_, None) => (Level::Installed, vec![], Some("not probed".into())),
        };
        let level = if tested.iter().any(|t| t.variant == b.variant) {
            Level::InferenceTested
        } else {
            level
        };
        backends.push(BackendStatus {
            backend: b.backend,
            variant: b.variant.clone(),
            level,
            devices,
            note,
        });
    }
    let adapters = report.adapters_result();
    let adapters_detected = adapters
        .as_ref()
        .map(|a| a.iter().filter(|x| !x.software).count())
        .unwrap_or(0);
    let plan_r = plan(&adapters, &report.builds, &report.probes, &needs, ovr).map_err(|e| e.to_string());

    let mut warnings = Vec::new();
    let verdict = match &plan_r {
        Ok(p) => match p.candidates.first() {
            Some(c) if c.backend != Backend::Cpu && c.device.is_some() => {
                let device = c.device.as_ref().map(|d| d.id.clone()).unwrap_or_default();
                match c.gpu_layers {
                    crate::select::GpuLayers::All => Verdict::Gpu {
                        backend: c.backend,
                        device,
                    },
                    _ => Verdict::PartialGpu {
                        backend: c.backend,
                        device,
                    },
                }
            }
            Some(_) => Verdict::CpuOnly,
            None => Verdict::NoBuild,
        },
        Err(_) => Verdict::NoBuild,
    };
    if verdict == Verdict::CpuOnly {
        warnings.push("No usable GPU backend: the model will run on the CPU (slow mode).".into());
    }
    if let Some(m) = memory {
        let host_need = needs.weights_mib + needs.mmproj_mib + needs.kv_mib + 1024;
        if matches!(verdict, Verdict::CpuOnly | Verdict::PartialGpu { .. }) && m.total_mib < host_need {
            warnings.push(format!(
                "Model needs about {host_need} MiB of RAM on the CPU path; this machine has {} MiB.",
                m.total_mib
            ));
        }
    }
    if adapters_detected > 0
        && !backends
            .iter()
            .any(|b| b.backend != Backend::Cpu && b.level >= Level::Verified)
    {
        warnings.push("A GPU is present but no installed GPU build initialised on it.".into());
    }
    HardwareCheck {
        os: report.os,
        arch: report.arch,
        cpu,
        memory,
        adapters_detected,
        backends,
        needs,
        needs_estimated,
        plan: plan_r,
        verdict,
        warnings,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SmokeError {
    #[error("model call failed: {0}")]
    Client(ClientError),
    #[error("empty answer")]
    Empty,
}

/// One tiny, fixed request to prove a model really runs on the selected backend. The prompt has
/// no user content.
pub fn smoke_test(model: &dyn ChatModel, cancel: &CancelToken) -> Result<u64, SmokeError> {
    let t = Instant::now();
    let r = model
        .chat(
            &ChatRequest {
                system: tsr_core::Sensitive::new("Reply with the single word: ready".into()),
                user: tsr_core::Sensitive::new("Are you ready?".into()),
                image_png: None,
                temperature: 0.0,
                max_tokens: 4,
                json_schema: None,
                stream: false,
                seed: Some(1),
            },
            cancel,
        )
        .map_err(SmokeError::Client)?;
    if r.content.expose().trim().is_empty() {
        return Err(SmokeError::Empty);
    }
    Ok(t.elapsed().min(Duration::from_secs(3600)).as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{InstalledBuild, ProbeError};
    use crate::hardware::{Adapter, GpuVendor};
    use std::path::PathBuf;

    fn build(variant: &str, b: Backend) -> InstalledBuild {
        InstalledBuild {
            variant: variant.into(),
            backend: b,
            dir: PathBuf::from("x"),
            server_exe: PathBuf::from("x/llama-server.exe"),
        }
    }

    fn report(adapters: Vec<Adapter>, probes: Vec<(String, Result<Vec<BackendDevice>, ProbeError>)>) -> HardwareReport {
        HardwareReport {
            os: "windows",
            arch: "x64",
            adapters: Ok(adapters),
            builds: vec![build("cpu-x64", Backend::Cpu), build("vulkan-x64", Backend::Vulkan)],
            build_notes: vec![],
            probes,
        }
    }

    fn amd_igpu() -> Adapter {
        Adapter {
            index: 0,
            name: "AMD Radeon(TM) Graphics".into(),
            vendor: GpuVendor::Amd,
            vendor_id: 0x1002,
            device_id: 1,
            dedicated_mib: 512,
            shared_mib: 7000,
            software: false,
        }
    }

    fn needs() -> ModelNeeds {
        estimated_needs(&crate::models::MANIFEST)
    }

    #[test]
    fn levels_are_kept_apart() {
        let dev = BackendDevice {
            id: "Vulkan0".into(),
            name: "AMD Radeon(TM) Graphics".into(),
            total_mib: Some(8096),
            free_mib: Some(7691),
        };
        let r = report(vec![amd_igpu()], vec![("vulkan-x64".into(), Ok(vec![dev]))]);
        let hc = hardware_check(&r, cpu_info(), None, needs(), true, &[], &Override::default());
        let vk = hc.backends.iter().find(|b| b.backend == Backend::Vulkan).unwrap();
        assert_eq!(vk.level, Level::Verified, "probe ok, but no model has run yet");
        let tested = [InferenceRecord {
            variant: "vulkan-x64".into(),
            device: Some("Vulkan0".into()),
            model_id: "qwen3-vl-4b-instruct-q4_k_m".into(),
            first_token_ms: 900,
        }];
        let hc = hardware_check(&r, cpu_info(), None, needs(), true, &tested, &Override::default());
        assert_eq!(
            hc.backends.iter().find(|b| b.backend == Backend::Vulkan).unwrap().level,
            Level::InferenceTested
        );
        assert!(
            matches!(
                hc.verdict,
                Verdict::Gpu {
                    backend: Backend::Vulkan,
                    ..
                }
            ),
            "{:?}",
            hc.verdict
        );
    }

    #[test]
    fn no_gpu_is_cpu_only_not_a_rejection() {
        let r = report(vec![], vec![("vulkan-x64".into(), Err(ProbeError::Exit(Some(1))))]);
        let hc = hardware_check(
            &r,
            cpu_info(),
            Some(Memory {
                total_mib: 16_000,
                available_mib: 8_000,
            }),
            needs(),
            true,
            &[],
            &Override::default(),
        );
        assert_eq!(hc.verdict, Verdict::CpuOnly);
        assert!(hc.warnings.iter().any(|w| w.contains("slow mode")));
        assert!(hc.plan.unwrap().candidates.last().unwrap().backend == Backend::Cpu);
    }

    #[test]
    fn gpu_present_but_backend_failing_is_reported() {
        let r = report(vec![amd_igpu()], vec![("vulkan-x64".into(), Err(ProbeError::Timeout))]);
        let hc = hardware_check(&r, cpu_info(), None, needs(), true, &[], &Override::default());
        assert!(
            hc.warnings
                .iter()
                .any(|w| w.contains("no installed GPU build initialised"))
        );
        assert_eq!(hc.verdict, Verdict::CpuOnly);
    }

    #[test]
    fn small_ram_on_cpu_path_warns() {
        let r = report(vec![], vec![]);
        let hc = hardware_check(
            &r,
            cpu_info(),
            Some(Memory {
                total_mib: 2_048,
                available_mib: 1_000,
            }),
            needs(),
            true,
            &[],
            &Override::default(),
        );
        assert!(hc.warnings.iter().any(|w| w.contains("RAM")), "{:?}", hc.warnings);
    }

    #[test]
    fn manifest_estimate_matches_published_sizes() {
        let n = needs();
        assert_eq!(n.weights_mib, 2497281664 >> 20);
        assert_eq!(n.mmproj_mib, 836180256 >> 20);
    }

    #[test]
    fn real_cpu_and_memory_queries_work() {
        let c = cpu_info();
        println!("REAL cpu: {c:?} memory: {:?}", memory());
        assert!(c.logical_cores >= 1);
        #[cfg(windows)]
        assert!(memory().is_some());
    }
}
