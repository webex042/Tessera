//! OS-level GPU enumeration (plan §7.1 "Hardware check").
//!
//! This layer only says which adapters exist, their vendor and memory. It needs no GPU runtime
//! (no CUDA, ROCm, Vulkan or Metal library is loaded). Whether a compute backend can actually use
//! an adapter is decided later by probing a pinned llama.cpp build (`backend::BackendProbe`).
//!
//! - Windows: DXGI (`CreateDXGIFactory1` → `EnumAdapters1`), vendor-neutral.
//! - Linux: `/sys/class/drm/card*/device/{vendor,device}` (+ `mem_info_vram_total` for amdgpu).
//! - macOS on Apple Silicon: one Apple GPU (unified memory; size comes from the Metal probe).
//! - Anything else: no adapters (CPU only).

use std::path::Path;

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub enum GpuVendor {
    Nvidia,
    Amd,
    Intel,
    Apple,
    Qualcomm,
    /// Microsoft Basic Render Driver / WARP (software rasteriser).
    Microsoft,
    Other(u32),
}

impl GpuVendor {
    pub fn from_pci_id(id: u32) -> Self {
        match id {
            0x10DE => GpuVendor::Nvidia,
            0x1002 | 0x1022 => GpuVendor::Amd,
            0x8086 => GpuVendor::Intel,
            0x106B => GpuVendor::Apple,
            // PCI id, and the ACPI "QCOM" id DXGI reports on Windows on Arm.
            0x5143 | 0x4D4F_4351 => GpuVendor::Qualcomm,
            0x1414 => GpuVendor::Microsoft,
            other => GpuVendor::Other(other),
        }
    }

    /// Best-effort vendor from a device name (backend probes report names, not PCI ids).
    pub fn from_name(name: &str) -> Option<Self> {
        let n = name.to_ascii_lowercase();
        let has = |w: &[&str]| w.iter().any(|x| n.contains(x));
        if has(&["nvidia", "geforce", "quadro", "tesla", "rtx ", "gtx "]) {
            Some(GpuVendor::Nvidia)
        } else if has(&["amd", "radeon", "ati "]) {
            Some(GpuVendor::Amd)
        } else if has(&["intel", "arc(tm)", " arc ", "iris", "uhd graphics"]) {
            Some(GpuVendor::Intel)
        } else if has(&["apple", "m1", "m2", "m3", "m4"]) && !has(&["amd"]) {
            Some(GpuVendor::Apple)
        } else if has(&["adreno", "qualcomm", "snapdragon"]) {
            Some(GpuVendor::Qualcomm)
        } else if has(&["microsoft basic", "llvmpipe", "swiftshader", "lavapipe"]) {
            Some(GpuVendor::Microsoft)
        } else {
            None
        }
    }

    pub fn as_str(self) -> String {
        match self {
            GpuVendor::Nvidia => "nvidia".into(),
            GpuVendor::Amd => "amd".into(),
            GpuVendor::Intel => "intel".into(),
            GpuVendor::Apple => "apple".into(),
            GpuVendor::Qualcomm => "qualcomm".into(),
            GpuVendor::Microsoft => "microsoft".into(),
            GpuVendor::Other(id) => format!("other(0x{id:04x})"),
        }
    }
}

/// One display adapter as the OS reports it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Adapter {
    pub index: u32,
    pub name: String,
    pub vendor: GpuVendor,
    pub vendor_id: u32,
    pub device_id: u32,
    /// Memory reserved for the GPU. Integrated GPUs report little here and use shared memory.
    pub dedicated_mib: u64,
    pub shared_mib: u64,
    /// Software rasterisers (WARP, llvmpipe) are never used for inference.
    pub software: bool,
}

impl Adapter {
    /// Integrated GPUs have (almost) no dedicated memory of their own.
    pub fn is_integrated(&self) -> bool {
        self.dedicated_mib < 1024 || self.vendor == GpuVendor::Apple
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("GPU enumeration failed: {0}")]
pub struct DetectError(pub String);

/// Where adapter facts come from. Tests use a mock; the app uses [`SystemAdapters`].
pub trait AdapterSource {
    fn adapters(&self) -> Result<Vec<Adapter>, DetectError>;
}

pub struct SystemAdapters;

impl AdapterSource for SystemAdapters {
    fn adapters(&self) -> Result<Vec<Adapter>, DetectError> {
        #[cfg(windows)]
        {
            dxgi_adapters()
        }
        #[cfg(target_os = "linux")]
        {
            sysfs_adapters(Path::new("/sys/class/drm"))
        }
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            Ok(vec![Adapter {
                index: 0,
                name: "Apple GPU".into(),
                vendor: GpuVendor::Apple,
                vendor_id: 0x106B,
                device_id: 0,
                dedicated_mib: 0,
                shared_mib: 0,
                software: false,
            }])
        }
        #[cfg(not(any(windows, target_os = "linux", all(target_os = "macos", target_arch = "aarch64"))))]
        {
            Ok(Vec::new())
        }
    }
}

#[cfg(windows)]
fn dxgi_adapters() -> Result<Vec<Adapter>, DetectError> {
    use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE, IDXGIFactory1};

    // SAFETY: plain DXGI factory/adapter queries; no resources are created on the GPU.
    let factory: IDXGIFactory1 =
        unsafe { CreateDXGIFactory1() }.map_err(|e| DetectError(format!("CreateDXGIFactory1 {:#010x}", e.code().0)))?;
    let mut out = Vec::new();
    for i in 0.. {
        let Ok(adapter) = (unsafe { factory.EnumAdapters1(i) }) else {
            break;
        };
        let Ok(d) = (unsafe { adapter.GetDesc1() }) else {
            continue;
        };
        let len = d
            .Description
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(d.Description.len());
        out.push(Adapter {
            index: i,
            name: String::from_utf16_lossy(&d.Description[..len]),
            vendor: GpuVendor::from_pci_id(d.VendorId),
            vendor_id: d.VendorId,
            device_id: d.DeviceId,
            dedicated_mib: (d.DedicatedVideoMemory as u64) >> 20,
            shared_mib: (d.SharedSystemMemory as u64) >> 20,
            software: (d.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32) != 0,
        });
    }
    Ok(out)
}

/// Parse a Linux DRM sysfs tree (`/sys/class/drm` or a test copy).
pub fn sysfs_adapters(root: &Path) -> Result<Vec<Adapter>, DetectError> {
    let entries = std::fs::read_dir(root).map_err(|e| DetectError(format!("read {}: {}", root.display(), e.kind())))?;
    let mut names: Vec<String> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        // `card0` is a GPU; `card0-DP-1` is a connector of it.
        .filter(|n| n.starts_with("card") && n[4..].chars().all(|c| c.is_ascii_digit()) && n.len() > 4)
        .collect();
    names.sort();
    let hex = |p: &Path| -> Option<u32> {
        let s = std::fs::read_to_string(p).ok()?;
        u32::from_str_radix(s.trim().trim_start_matches("0x"), 16).ok()
    };
    let mut out = Vec::new();
    for (i, n) in names.iter().enumerate() {
        let dev = root.join(n).join("device");
        let Some(vendor_id) = hex(&dev.join("vendor")) else {
            continue;
        };
        let device_id = hex(&dev.join("device")).unwrap_or(0);
        let vram = std::fs::read_to_string(dev.join("mem_info_vram_total"))
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .map(|b| b >> 20)
            .unwrap_or(0);
        let vendor = GpuVendor::from_pci_id(vendor_id);
        out.push(Adapter {
            index: i as u32,
            name: format!("{} GPU {:04x}:{:04x}", vendor.as_str(), vendor_id, device_id),
            vendor,
            vendor_id,
            device_id,
            dedicated_mib: vram,
            shared_mib: 0,
            software: false,
        });
    }
    Ok(out)
}

/// Fixed adapter list for tests (SIMULATED hardware).
pub struct MockAdapters(pub Result<Vec<Adapter>, DetectError>);

impl AdapterSource for MockAdapters {
    fn adapters(&self) -> Result<Vec<Adapter>, DetectError> {
        self.0.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vendor_ids_and_names() {
        assert_eq!(GpuVendor::from_pci_id(0x10DE), GpuVendor::Nvidia);
        assert_eq!(GpuVendor::from_pci_id(0x1002), GpuVendor::Amd);
        assert_eq!(GpuVendor::from_pci_id(0x8086), GpuVendor::Intel);
        assert_eq!(GpuVendor::from_pci_id(0x4D4F_4351), GpuVendor::Qualcomm);
        assert_eq!(GpuVendor::from_name("AMD Radeon (TM) Graphics"), Some(GpuVendor::Amd));
        assert_eq!(
            GpuVendor::from_name("NVIDIA GeForce RTX 4050 Laptop GPU"),
            Some(GpuVendor::Nvidia)
        );
        assert_eq!(
            GpuVendor::from_name("Intel(R) Arc(TM) A770 Graphics"),
            Some(GpuVendor::Intel)
        );
        assert_eq!(GpuVendor::from_name("Apple M3 Pro"), Some(GpuVendor::Apple));
        assert_eq!(
            GpuVendor::from_name("Qualcomm(R) Adreno(TM) X1-85 GPU"),
            Some(GpuVendor::Qualcomm)
        );
        assert_eq!(GpuVendor::from_name("Mystery Accelerator"), None);
    }

    /// SIMULATED: a fake sysfs tree with an AMD dGPU, an Intel iGPU and a connector entry.
    #[test]
    fn linux_sysfs_parsing() {
        let d = tempfile::tempdir().unwrap();
        let mk = |card: &str, vendor: &str, device: &str, vram: Option<&str>| {
            let p = d.path().join(card).join("device");
            std::fs::create_dir_all(&p).unwrap();
            std::fs::write(p.join("vendor"), vendor).unwrap();
            std::fs::write(p.join("device"), device).unwrap();
            if let Some(v) = vram {
                std::fs::write(p.join("mem_info_vram_total"), v).unwrap();
            }
        };
        mk("card0", "0x1002\n", "0x744c\n", Some("25753026560\n"));
        mk("card1", "0x8086\n", "0x46a6\n", None);
        std::fs::create_dir_all(d.path().join("card0-DP-1")).unwrap();
        let a = sysfs_adapters(d.path()).unwrap();
        assert_eq!(a.len(), 2);
        assert_eq!(a[0].vendor, GpuVendor::Amd);
        assert_eq!(a[0].dedicated_mib, 24560);
        assert!(!a[0].is_integrated());
        assert_eq!(a[1].vendor, GpuVendor::Intel);
        assert!(a[1].is_integrated());
    }

    #[test]
    fn missing_sysfs_is_a_detection_error() {
        assert!(sysfs_adapters(Path::new("/definitely/not/here")).is_err());
    }

    /// REAL hardware: enumerate this machine's adapters through the OS. Never fails because of
    /// what hardware exists (CI runners may only have a software adapter); it fails only if the
    /// OS query itself errors.
    #[test]
    fn real_system_enumeration_works() {
        let a = SystemAdapters.adapters().expect("OS adapter query");
        for x in &a {
            println!(
                "REAL adapter {}: {} vendor={} dedicated={} MiB shared={} MiB software={}",
                x.index,
                x.name,
                x.vendor.as_str(),
                x.dedicated_mib,
                x.shared_mib,
                x.software
            );
        }
    }
}
