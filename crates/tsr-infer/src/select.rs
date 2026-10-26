//! Backend selection, manual override and fallback (plan §7.1).
//!
//! Input: OS adapters, verified llama.cpp builds, what each build's backend probe reported, the
//! model's needs and an optional user override. Output: an ordered list of launch candidates —
//! best GPU option first, CPU always last (unless the user forbids fallback) — plus content-free
//! notes explaining every option that was skipped. [`launch_with_fallback`] then tries the
//! candidates in order and records why each failed.
//!
//! Selection never changes the model, its quantisation or the KV-cache type. Moving work between
//! backends can change floating-point results slightly; evals are therefore recorded per backend
//! (docs/hardware-backends.md).

use std::cmp::Reverse;
use std::collections::BTreeSet;
use std::path::PathBuf;

use serde::Serialize;

use crate::backend::{Backend, BackendDevice, InstalledBuild, ProbeError, Support, tensor_support};
use crate::gguf::{GgmlType, GgufInfo};
use crate::hardware::{Adapter, DetectError, GpuVendor};

/// Compute buffers for text + vision (plan §7.2: 0.5–1.0 GB). Kept on the high side.
pub const OVERHEAD_MIB: u64 = 768;

/// What the model needs, from its GGUF headers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ModelNeeds {
    pub weights_mib: u64,
    pub mmproj_mib: u64,
    pub kv_mib: u64,
    pub n_layers: u32,
    pub tensor_types: BTreeSet<GgmlType>,
}

impl ModelNeeds {
    /// `kv_bytes_per_elem`: 2.0 for f16, 1.0625 for q8_0.
    pub fn from_gguf(model: &GgufInfo, mmproj: Option<&GgufInfo>, ctx: u64, kv_bytes_per_elem: f64) -> Self {
        let mut tensor_types: BTreeSet<GgmlType> = model.tensor_types.keys().copied().collect();
        if let Some(m) = mmproj {
            tensor_types.extend(m.tensor_types.keys().copied());
        }
        ModelNeeds {
            weights_mib: model.file_size >> 20,
            mmproj_mib: mmproj.map_or(0, |m| m.file_size >> 20),
            // Unknown attention shape: assume the plan's 8k/q8_0 estimate for a 4B model.
            kv_mib: model.kv_cache_mib(ctx, kv_bytes_per_elem).unwrap_or(640),
            n_layers: model.block_count().unwrap_or(32) as u32,
            tensor_types,
        }
    }

    fn full_mib(&self) -> u64 {
        self.weights_mib + self.mmproj_mib + self.kv_mib + OVERHEAD_MIB
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum DeviceSelector {
    /// Exact backend device id, e.g. `Vulkan1`, `CUDA0`.
    Id(String),
    /// The N-th device of the chosen backend (`Vulkan1` is index 1).
    Index(u32),
    /// Case-insensitive substring of the device name, e.g. `rtx 4050`.
    Name(String),
}

/// User override from settings (`inference.backend`, `inference.device`, `inference.gpu_layers`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Override {
    pub backend: Option<Backend>,
    pub device: Option<DeviceSelector>,
    pub gpu_layers: Option<u32>,
    /// If the forced choice fails, may Tessara fall back (to other GPUs, then CPU)?
    pub allow_fallback: bool,
}

impl Default for Override {
    fn default() -> Self {
        Override {
            backend: None,
            device: None,
            gpu_layers: None,
            allow_fallback: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SelectError {
    #[error("unknown backend '{0}' (expected auto, cuda, rocm, vulkan, sycl, openvino, metal, opencl or cpu)")]
    UnknownBackend(String),
    #[error("gpu_layers must be a number")]
    BadGpuLayers,
    #[error("no verified llama.cpp build is installed")]
    NoBuildInstalled,
    #[error("override cannot be satisfied and fallback is disabled: {0}")]
    OverrideUnavailable(String),
}

impl Override {
    /// Parse settings strings. `backend = "auto"` (or empty) means automatic selection.
    pub fn parse(
        backend: Option<&str>,
        device: Option<&str>,
        gpu_layers: Option<&str>,
        allow_fallback: bool,
    ) -> Result<Self, SelectError> {
        let backend = match backend
            .map(str::trim)
            .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("auto"))
        {
            None => None,
            Some(s) => Some(Backend::parse(s).ok_or_else(|| SelectError::UnknownBackend(s.to_owned()))?),
        };
        let device = device.map(str::trim).filter(|s| !s.is_empty()).map(|s| {
            if let Ok(n) = s.parse::<u32>() {
                DeviceSelector::Index(n)
            } else if s.chars().last().is_some_and(|c| c.is_ascii_digit()) && !s.contains(' ') {
                DeviceSelector::Id(s.to_owned())
            } else {
                DeviceSelector::Name(s.to_owned())
            }
        });
        let gpu_layers = gpu_layers
            .map(|s| s.trim().parse::<u32>().map_err(|_| SelectError::BadGpuLayers))
            .transpose()?;
        Ok(Override {
            backend,
            device,
            gpu_layers,
            allow_fallback,
        })
    }

    fn device_matches(&self, d: &BackendDevice) -> bool {
        match &self.device {
            None => true,
            Some(DeviceSelector::Id(id)) => d.id.eq_ignore_ascii_case(id),
            Some(DeviceSelector::Index(n)) => {
                d.id.ends_with(&n.to_string())
                    && d.id[..d.id.len() - n.to_string().len()]
                        .chars()
                        .all(|c| !c.is_ascii_digit())
            }
            Some(DeviceSelector::Name(s)) => d.name.to_lowercase().contains(&s.to_lowercase()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum GpuLayers {
    All,
    Count(u32),
    None,
}

/// One way to start the sidecar.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Candidate {
    pub backend: Backend,
    pub variant: String,
    pub server_exe: PathBuf,
    pub device: Option<BackendDevice>,
    pub vendor: Option<GpuVendor>,
    pub gpu_layers: GpuLayers,
    /// Run the vision encoder (mmproj) on the device; `false` keeps it on the CPU.
    pub mmproj_offload: bool,
    pub reason: String,
}

impl Candidate {
    /// The device-related llama-server arguments (the rest is in the plan §7.1 command line).
    pub fn llama_args(&self) -> Vec<String> {
        let mut a = Vec::new();
        match (&self.device, self.gpu_layers) {
            (Some(d), GpuLayers::All) => a.extend(["--device".into(), d.id.clone(), "-ngl".into(), "all".into()]),
            (Some(d), GpuLayers::Count(n)) => a.extend(["--device".into(), d.id.clone(), "-ngl".into(), n.to_string()]),
            _ => a.extend(["--device".into(), "none".into(), "-ngl".into(), "0".into()]),
        }
        if !self.mmproj_offload {
            a.push("--no-mmproj-offload".into());
        }
        a
    }

    pub fn label(&self) -> String {
        match &self.device {
            Some(d) => format!("{} on {} ({})", self.backend.as_str(), d.id, d.name),
            None => format!("{} (no GPU offload)", self.backend.as_str()),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct SelectionPlan {
    pub candidates: Vec<Candidate>,
    /// Why options were skipped or adjusted. Hardware facts only, never user content.
    pub notes: Vec<String>,
}

/// Preferred backends per vendor, best first. A combination not listed is not used
/// automatically (it can still be forced with an override).
pub fn preference(vendor: GpuVendor, backend: Backend) -> Option<usize> {
    let order: &[Backend] = match vendor {
        GpuVendor::Nvidia => &[Backend::Cuda, Backend::Vulkan],
        GpuVendor::Amd => &[Backend::Rocm, Backend::Vulkan],
        GpuVendor::Intel => &[Backend::Sycl, Backend::Vulkan, Backend::OpenVino],
        GpuVendor::Apple => &[Backend::Metal],
        GpuVendor::Qualcomm => &[Backend::OpenCl, Backend::Vulkan],
        GpuVendor::Microsoft => &[],
        GpuVendor::Other(_) => &[Backend::Vulkan],
    };
    order.iter().position(|b| *b == backend)
}

fn norm(s: &str) -> String {
    s.to_lowercase()
        .replace("(tm)", "")
        .replace("(r)", "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn match_adapter<'a>(dev: &BackendDevice, adapters: &'a [Adapter]) -> Option<&'a Adapter> {
    let n = norm(&dev.name);
    let hw: Vec<&Adapter> = adapters.iter().filter(|a| !a.software).collect();
    hw.iter()
        .find(|a| {
            let an = norm(&a.name);
            an == n || an.contains(&n) || n.contains(&an)
        })
        .copied()
        .or_else(|| {
            // Same vendor and only one such adapter: that is the one.
            let v = GpuVendor::from_name(&dev.name)?;
            let same: Vec<&&Adapter> = hw.iter().filter(|a| a.vendor == v).collect();
            (same.len() == 1).then(|| *same[0])
        })
}

struct Option_ {
    cand: Candidate,
    integrated: bool,
    pref: usize,
    free: u64,
    full_fit: bool,
}

pub fn plan(
    adapters: &Result<Vec<Adapter>, DetectError>,
    builds: &[InstalledBuild],
    probes: &[(String, Result<Vec<BackendDevice>, ProbeError>)],
    model: &ModelNeeds,
    ovr: &Override,
) -> Result<SelectionPlan, SelectError> {
    let mut notes = Vec::new();
    let mut note = |level: tracing::Level, msg: String| {
        if level == tracing::Level::WARN {
            tracing::warn!(note = %msg, "backend selection");
        } else {
            tracing::info!(note = %msg, "backend selection");
        }
        notes.push(msg);
    };
    if builds.is_empty() {
        return Err(SelectError::NoBuildInstalled);
    }
    let adapter_list: &[Adapter] = match adapters {
        Ok(a) => a,
        Err(e) => {
            note(tracing::Level::WARN, format!("{e}; using backend probes only"));
            &[]
        }
    };

    let mut options: Vec<Option_> = Vec::new();
    for build in builds.iter().filter(|b| b.backend != Backend::Cpu) {
        let be = build.backend;
        let devices = match probes.iter().find(|(v, _)| v == &build.variant).map(|(_, r)| r) {
            Some(Ok(d)) => d,
            Some(Err(e)) => {
                note(
                    tracing::Level::WARN,
                    format!("{} backend unavailable: {e}", be.as_str()),
                );
                continue;
            }
            None => {
                note(tracing::Level::INFO, format!("{} backend not probed", be.as_str()));
                continue;
            }
        };
        if devices.is_empty() {
            note(
                tracing::Level::INFO,
                format!("{} backend initialised but found no device", be.as_str()),
            );
        }
        for dev in devices {
            let adapter = match_adapter(dev, adapter_list);
            if adapter.is_some_and(|a| a.software) {
                continue;
            }
            let vendor = adapter.map(|a| a.vendor).or_else(|| GpuVendor::from_name(&dev.name));
            let forced = ovr.backend == Some(be);
            if forced && !ovr.device_matches(dev) {
                continue;
            }
            if !forced && ovr.backend.is_some() {
                continue;
            }
            if !ovr.device_matches(dev) {
                continue;
            }
            if matches!(vendor, Some(GpuVendor::Microsoft)) {
                note(tracing::Level::INFO, format!("{}: software renderer, skipped", dev.id));
                continue;
            }
            let pref = match vendor.and_then(|v| preference(v, be)) {
                Some(p) => p,
                None if forced => {
                    note(
                        tracing::Level::WARN,
                        format!(
                            "{}: {} is not a tested pairing for this vendor; used because it was forced",
                            dev.id,
                            be.as_str()
                        ),
                    );
                    9
                }
                None => {
                    note(
                        tracing::Level::INFO,
                        format!(
                            "{}: {} not used automatically for vendor {}",
                            dev.id,
                            be.as_str(),
                            vendor.map_or("unknown".into(), |v| v.as_str())
                        ),
                    );
                    continue;
                }
            };
            // Model/backend compatibility. Never requantise or swap the model to make it fit.
            let unverified: Vec<String> = model
                .tensor_types
                .iter()
                .filter(|t| tensor_support(be, **t) == Support::Unverified)
                .map(|t| format!("{t:?}"))
                .collect();
            if !unverified.is_empty() {
                if forced {
                    note(
                        tracing::Level::WARN,
                        format!(
                            "{}: model types {} are unverified on {}; used because it was forced",
                            dev.id,
                            unverified.join(","),
                            be.as_str()
                        ),
                    );
                } else {
                    note(
                        tracing::Level::INFO,
                        format!(
                            "{}: model types {} are unverified on {}; skipped (force with override)",
                            dev.id,
                            unverified.join(","),
                            be.as_str()
                        ),
                    );
                    continue;
                }
            }
            // Memory fit.
            let free = dev
                .free_mib
                .or(dev.total_mib)
                .or(adapter.map(|a| a.dedicated_mib).filter(|m| *m > 0));
            let (layers, mmproj_offload, full_fit, why) = match (free, ovr.gpu_layers) {
                (_, Some(0)) => {
                    note(
                        tracing::Level::INFO,
                        format!("{}: gpu_layers=0 in override; no offload", dev.id),
                    );
                    continue;
                }
                (_, Some(n)) => (GpuLayers::Count(n), true, true, format!("gpu_layers={n} from override")),
                (None, None) => (
                    GpuLayers::All,
                    true,
                    true,
                    "memory not reported; relying on initialisation".to_string(),
                ),
                (Some(f), None) if f >= model.full_mib() => (
                    GpuLayers::All,
                    true,
                    true,
                    format!("fits: need {} MiB, free {f} MiB", model.full_mib()),
                ),
                (Some(f), None) if f >= model.full_mib() - model.mmproj_mib => (
                    GpuLayers::All,
                    false,
                    true,
                    format!(
                        "fits without vision encoder: need {} MiB, free {f} MiB; mmproj on CPU",
                        model.full_mib()
                    ),
                ),
                (Some(f), None) => {
                    let budget = f.saturating_sub(model.kv_mib + OVERHEAD_MIB);
                    let per_layer = (model.weights_mib / model.n_layers.max(1) as u64).max(1);
                    let n = (budget / per_layer).min(model.n_layers as u64) as u32;
                    if n >= (model.n_layers / 4).max(1) {
                        (
                            GpuLayers::Count(n),
                            false,
                            false,
                            format!(
                                "partial offload {n}/{} layers: need {} MiB, free {f} MiB",
                                model.n_layers,
                                model.full_mib()
                            ),
                        )
                    } else {
                        note(
                            tracing::Level::INFO,
                            format!(
                                "{}: insufficient memory (need {} MiB, free {f} MiB)",
                                dev.id,
                                model.full_mib()
                            ),
                        );
                        continue;
                    }
                }
            };
            let integrated = adapter.map(Adapter::is_integrated).unwrap_or(false);
            options.push(Option_ {
                cand: Candidate {
                    backend: be,
                    variant: build.variant.clone(),
                    server_exe: build.server_exe.clone(),
                    device: Some(dev.clone()),
                    vendor,
                    gpu_layers: layers,
                    mmproj_offload,
                    reason: why,
                },
                integrated,
                pref,
                free: free.unwrap_or(0),
                full_fit,
            });
        }
    }

    // Full fit first, then discrete before integrated, then the vendor's preferred backend, then
    // the device with the most free memory.
    options.sort_by_key(|o| (!o.full_fit, o.integrated, o.pref, Reverse(o.free)));
    let mut candidates: Vec<Candidate> = options.into_iter().map(|o| o.cand).collect();

    let cpu_forced = ovr.backend == Some(Backend::Cpu);
    let gpu_forced = ovr.backend.is_some() && !cpu_forced;
    if gpu_forced && candidates.is_empty() {
        let msg = format!(
            "override backend {} has no usable device",
            ovr.backend.map_or("", |b| b.as_str())
        );
        if !ovr.allow_fallback {
            return Err(SelectError::OverrideUnavailable(msg));
        }
        note(tracing::Level::WARN, format!("{msg}; falling back to CPU"));
    }
    if cpu_forced || ovr.allow_fallback || candidates.is_empty() {
        // Any verified build can run on the CPU; prefer the plain CPU build.
        let b = builds.iter().find(|b| b.backend == Backend::Cpu).unwrap_or(&builds[0]);
        let reason = if cpu_forced {
            "CPU forced by override".to_string()
        } else if candidates.is_empty() {
            "no usable GPU; CPU only".to_string()
        } else {
            "last-resort fallback".to_string()
        };
        candidates.push(Candidate {
            backend: Backend::Cpu,
            variant: b.variant.clone(),
            server_exe: b.server_exe.clone(),
            device: None,
            vendor: None,
            gpu_layers: GpuLayers::None,
            mmproj_offload: false,
            reason,
        });
    }
    if let Some(first) = candidates.first() {
        tracing::info!(backend = first.backend.as_str(), device = %first.label(), reason = %first.reason,
                       fallbacks = candidates.len() - 1, "backend selected");
    }
    Ok(SelectionPlan { candidates, notes })
}

/// Starts the sidecar for one candidate (real implementation: Stage 2 sidecar lifecycle).
pub trait Launcher {
    type Running;
    /// Errors must be content-free (exit codes, timeouts, health-check status).
    fn launch(&mut self, c: &Candidate) -> Result<Self::Running, String>;
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Attempt {
    pub candidate: String,
    pub error: String,
}

pub struct Launched<R> {
    pub running: R,
    pub candidate: Candidate,
    pub failed_attempts: Vec<Attempt>,
}

/// Try candidates in order; each failure is logged with its reason and the next one is tried.
pub fn launch_with_fallback<L: Launcher>(
    plan: &SelectionPlan,
    launcher: &mut L,
) -> Result<Launched<L::Running>, Vec<Attempt>> {
    let mut failed = Vec::new();
    for c in &plan.candidates {
        match launcher.launch(c) {
            Ok(running) => {
                if !failed.is_empty() {
                    tracing::warn!(selected = %c.label(), failed = failed.len(), "started after fallback");
                }
                return Ok(Launched {
                    running,
                    candidate: c.clone(),
                    failed_attempts: failed,
                });
            }
            Err(e) => {
                tracing::warn!(candidate = %c.label(), error = %e, "backend initialisation failed; trying next");
                failed.push(Attempt {
                    candidate: c.label(),
                    error: e,
                });
            }
        }
    }
    Err(failed)
}
