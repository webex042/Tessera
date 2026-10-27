//! Tessara inference layer (plan §7).
//!
//! GPU-vendor-independent by design: nothing here links to CUDA, ROCm, Vulkan, Metal or any
//! other GPU library. Backends live in separately pinned llama.cpp builds (one per backend), and
//! which one runs is decided at run time:
//!
//! 1. [`hardware`] — which adapters the OS reports (DXGI / sysfs / Apple), no GPU runtime needed.
//! 2. [`backend`] — which pinned builds are installed and intact, and what each one's backend
//!    actually initialises on this machine (`llama-server --list-devices`).
//! 3. [`gguf`] — the model's format, quantisation types and size.
//! 4. [`select`] — rank the options, apply the user's override, always end with CPU, and try
//!    candidates in order until one starts.
//!
//! Running the model (Stage 2):
//!
//! 5. [`sidecar`] — start one contained llama-server with a cleared environment, an authenticated
//!    loopback endpoint and a readiness check; [`supervisor`] — its lifecycle state machine
//!    (health, restart with backoff, restart budget, idle unload).
//! 6. [`http`] / [`client`] — the minimal loopback-only HTTP client and the chat API on top.
//! 7. [`sched`] — the priority scheduler with preemption (pure state machine); [`runtime`] — the
//!    threads that drive it; [`metrics`] — content-free counters and histograms.
//! 8. [`prompts`] — the versioned templates from `prompts/` (plan §16), with content escaping.

pub mod backend;
pub mod cancel;
pub mod client;
pub mod gguf;
pub mod hardware;
pub mod http;
pub mod metrics;
pub mod models;
pub mod onboard;
pub mod prompts;
pub mod runtime;
pub mod sched;
pub mod select;
pub mod sidecar;
pub mod supervisor;

use std::path::Path;

use serde::Serialize;

use backend::{BackendDevice, BackendProbe, BuildNote, InstalledBuild, ProbeError};
use hardware::{Adapter, AdapterSource, DetectError};

/// Everything detection found, for logs and the diagnostics page. Hardware facts only.
#[derive(Debug, Serialize)]
pub struct HardwareReport {
    pub os: &'static str,
    pub arch: &'static str,
    pub adapters: Result<Vec<Adapter>, String>,
    pub builds: Vec<InstalledBuild>,
    pub build_notes: Vec<BuildNote>,
    pub probes: Vec<(String, Result<Vec<BackendDevice>, ProbeError>)>,
}

/// Run detection: OS adapters, verified builds under `third_party`, and one probe per build.
/// The plain CPU build is not probed (it has no devices to list).
pub fn detect(third_party: &Path, adapters: &dyn AdapterSource, probe: &dyn BackendProbe) -> HardwareReport {
    let found: Result<Vec<Adapter>, DetectError> = adapters.adapters();
    if let Ok(a) = &found {
        for x in a {
            tracing::info!(index = x.index, adapter_name = %x.name, vendor = %x.vendor.as_str(), dedicated_mib = x.dedicated_mib,
                           shared_mib = x.shared_mib, software = x.software, "gpu adapter");
        }
    }
    let (builds, build_notes) = backend::installed_builds(third_party);
    let probes = builds
        .iter()
        .filter(|b| b.backend != backend::Backend::Cpu)
        .map(|b| (b.variant.clone(), probe.list_devices(b)))
        .collect();
    HardwareReport {
        os: backend::current_os(),
        arch: backend::current_arch(),
        adapters: found.map_err(|e| e.to_string()),
        builds,
        build_notes,
        probes,
    }
}

impl HardwareReport {
    pub fn adapters_result(&self) -> Result<Vec<Adapter>, DetectError> {
        self.adapters.clone().map_err(DetectError)
    }
}
