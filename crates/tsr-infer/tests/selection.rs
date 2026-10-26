//! SIMULATED hardware: every test here uses mocked adapter lists and mocked backend probe output
//! (in the exact `--list-devices` format the real builds print). Nothing in this file touches a GPU.
//! Real-hardware checks are in `tests/real_hardware.rs`.

use std::collections::BTreeSet;
use std::path::PathBuf;

use tsr_infer::backend::{Backend, BackendDevice, InstalledBuild, ProbeError};
use tsr_infer::gguf::GgmlType;
use tsr_infer::hardware::{Adapter, DetectError, GpuVendor};
use tsr_infer::select::{
    Candidate, DeviceSelector, GpuLayers, Launcher, ModelNeeds, Override, SelectError, launch_with_fallback, plan,
};

fn build(variant: &str, backend: Backend) -> InstalledBuild {
    InstalledBuild {
        variant: variant.into(),
        backend,
        dir: PathBuf::from(format!("third_party/llama_cpp/bX/{variant}")),
        server_exe: PathBuf::from(format!("third_party/llama_cpp/bX/{variant}/llama-server.exe")),
    }
}

fn adapter(index: u32, name: &str, vendor_id: u32, dedicated_mib: u64) -> Adapter {
    Adapter {
        index,
        name: name.into(),
        vendor: GpuVendor::from_pci_id(vendor_id),
        vendor_id,
        device_id: 0,
        dedicated_mib,
        shared_mib: 8000,
        software: false,
    }
}

fn dev(id: &str, name: &str, total: u64, free: u64) -> BackendDevice {
    BackendDevice {
        id: id.into(),
        name: name.into(),
        total_mib: Some(total),
        free_mib: Some(free),
    }
}

/// Qwen3-VL-4B Q4_K_M-like needs: 2500 + 800 + 600 + 768 overhead = 4668 MiB.
fn model() -> ModelNeeds {
    ModelNeeds {
        weights_mib: 2500,
        mmproj_mib: 800,
        kv_mib: 600,
        n_layers: 36,
        tensor_types: BTreeSet::from([GgmlType::Q4K, GgmlType::Q6K, GgmlType::F32, GgmlType::F16]),
    }
}

fn backends(c: &[Candidate]) -> Vec<String> {
    c.iter()
        .map(|c| {
            format!(
                "{}:{}",
                c.backend.as_str(),
                c.device.as_ref().map_or("-", |d| d.id.as_str())
            )
        })
        .collect()
}

const NV: u32 = 0x10DE;
const AMD: u32 = 0x1002;
const INTEL: u32 = 0x8086;

#[test]
fn no_gpu_available_runs_on_cpu() {
    let p = plan(
        &Ok(vec![]),
        &[build("cpu-x64", Backend::Cpu)],
        &[],
        &model(),
        &Override::default(),
    )
    .unwrap();
    assert_eq!(backends(&p.candidates), ["cpu:-"]);
    assert_eq!(p.candidates[0].reason, "no usable GPU; CPU only");
    assert_eq!(
        p.candidates[0].llama_args(),
        ["--device", "none", "-ngl", "0", "--no-mmproj-offload"]
    );
}

#[test]
fn nvidia_with_cuda_prefers_cuda_then_vulkan_then_cpu() {
    let a = Ok(vec![adapter(0, "NVIDIA GeForce RTX 4050 Laptop GPU", NV, 6141)]);
    let builds = [
        build("cuda-12.4-x64", Backend::Cuda),
        build("vulkan-x64", Backend::Vulkan),
        build("cpu-x64", Backend::Cpu),
    ];
    let probes = vec![
        (
            "cuda-12.4-x64".to_string(),
            Ok(vec![dev("CUDA0", "NVIDIA GeForce RTX 4050 Laptop GPU", 6141, 5800)]),
        ),
        (
            "vulkan-x64".to_string(),
            Ok(vec![dev("Vulkan0", "NVIDIA GeForce RTX 4050 Laptop GPU", 6141, 5800)]),
        ),
    ];
    let p = plan(&a, &builds, &probes, &model(), &Override::default()).unwrap();
    assert_eq!(backends(&p.candidates), ["cuda:CUDA0", "vulkan:Vulkan0", "cpu:-"]);
    assert_eq!(p.candidates[0].gpu_layers, GpuLayers::All);
    assert!(p.candidates[0].mmproj_offload);
    assert_eq!(p.candidates[0].llama_args(), ["--device", "CUDA0", "-ngl", "all"]);
}

#[test]
fn nvidia_without_cuda_build_uses_vulkan() {
    let a = Ok(vec![adapter(0, "NVIDIA GeForce RTX 3060", NV, 12288)]);
    let builds = [build("vulkan-x64", Backend::Vulkan), build("cpu-x64", Backend::Cpu)];
    let probes = vec![(
        "vulkan-x64".to_string(),
        Ok(vec![dev("Vulkan0", "NVIDIA GeForce RTX 3060", 12288, 11000)]),
    )];
    let p = plan(&a, &builds, &probes, &model(), &Override::default()).unwrap();
    assert_eq!(backends(&p.candidates), ["vulkan:Vulkan0", "cpu:-"]);
}

#[test]
fn amd_with_rocm_prefers_rocm() {
    let a = Ok(vec![adapter(0, "AMD Radeon RX 7900 XTX", AMD, 24560)]);
    let builds = [
        build("rocm-x64", Backend::Rocm),
        build("vulkan-x64", Backend::Vulkan),
        build("cpu-x64", Backend::Cpu),
    ];
    let probes = vec![
        (
            "rocm-x64".to_string(),
            Ok(vec![dev("ROCm0", "AMD Radeon RX 7900 XTX", 24560, 24000)]),
        ),
        (
            "vulkan-x64".to_string(),
            Ok(vec![dev("Vulkan0", "AMD Radeon RX 7900 XTX", 24560, 24000)]),
        ),
    ];
    let p = plan(&a, &builds, &probes, &model(), &Override::default()).unwrap();
    assert_eq!(backends(&p.candidates), ["rocm:ROCm0", "vulkan:Vulkan0", "cpu:-"]);
}

#[test]
fn amd_where_rocm_fails_to_initialise_uses_vulkan() {
    // e.g. an integrated Vega GPU, which ROCm does not support.
    let a = Ok(vec![adapter(0, "AMD Radeon (TM) Graphics", AMD, 512)]);
    let builds = [
        build("rocm-x64", Backend::Rocm),
        build("vulkan-x64", Backend::Vulkan),
        build("cpu-x64", Backend::Cpu),
    ];
    let probes = vec![
        ("rocm-x64".to_string(), Err(ProbeError::Exit(Some(1)))),
        (
            "vulkan-x64".to_string(),
            Ok(vec![dev("Vulkan0", "AMD Radeon (TM) Graphics", 8096, 7691)]),
        ),
    ];
    let p = plan(&a, &builds, &probes, &model(), &Override::default()).unwrap();
    assert_eq!(backends(&p.candidates), ["vulkan:Vulkan0", "cpu:-"]);
    assert!(
        p.notes.iter().any(|n| n.contains("rocm backend unavailable")),
        "{:?}",
        p.notes
    );
}

#[test]
fn intel_prefers_sycl_and_falls_back_to_vulkan() {
    let a = Ok(vec![adapter(0, "Intel(R) Arc(TM) A770 Graphics", INTEL, 16288)]);
    let builds = [
        build("sycl-x64", Backend::Sycl),
        build("vulkan-x64", Backend::Vulkan),
        build("cpu-x64", Backend::Cpu),
    ];
    let probes = vec![
        (
            "sycl-x64".to_string(),
            Ok(vec![dev("SYCL0", "Intel(R) Arc(TM) A770 Graphics", 16288, 15800)]),
        ),
        (
            "vulkan-x64".to_string(),
            Ok(vec![dev("Vulkan0", "Intel(R) Arc(TM) A770 Graphics", 16288, 15800)]),
        ),
    ];
    let p = plan(&a, &builds, &probes, &model(), &Override::default()).unwrap();
    assert_eq!(backends(&p.candidates), ["sycl:SYCL0", "vulkan:Vulkan0", "cpu:-"]);

    let builds = [build("vulkan-x64", Backend::Vulkan), build("cpu-x64", Backend::Cpu)];
    let probes = vec![(
        "vulkan-x64".to_string(),
        Ok(vec![dev("Vulkan0", "Intel(R) Arc(TM) A770 Graphics", 16288, 15800)]),
    )];
    let p = plan(&a, &builds, &probes, &model(), &Override::default()).unwrap();
    assert_eq!(backends(&p.candidates), ["vulkan:Vulkan0", "cpu:-"]);
}

#[test]
fn apple_silicon_uses_metal() {
    let a = Ok(vec![Adapter {
        index: 0,
        name: "Apple GPU".into(),
        vendor: GpuVendor::Apple,
        vendor_id: 0x106B,
        device_id: 0,
        dedicated_mib: 0,
        shared_mib: 0,
        software: false,
    }]);
    let builds = [build("macos-arm64", Backend::Metal)];
    let probes = vec![(
        "macos-arm64".to_string(),
        Ok(vec![dev("MTL0", "Apple M3 Pro", 27648, 27000)]),
    )];
    let p = plan(&a, &builds, &probes, &model(), &Override::default()).unwrap();
    assert_eq!(backends(&p.candidates), ["metal:MTL0", "cpu:-"]);
    // The Metal build also runs CPU-only when it has to.
    assert_eq!(p.candidates[1].variant, "macos-arm64");
}

#[test]
fn qualcomm_adreno_prefers_opencl() {
    let a = Ok(vec![adapter(0, "Qualcomm(R) Adreno(TM) X1-85 GPU", 0x4D4F_4351, 0)]);
    let builds = [
        build("opencl-adreno-arm64", Backend::OpenCl),
        build("cpu-arm64", Backend::Cpu),
    ];
    let probes = vec![(
        "opencl-adreno-arm64".to_string(),
        Ok(vec![dev("GPUOpenCL", "QUALCOMM Adreno(TM) X1-85", 8000, 7000)]),
    )];
    let mut m = model();
    m.tensor_types = BTreeSet::from([GgmlType::Q4_0, GgmlType::F16, GgmlType::F32]);
    let p = plan(&a, &builds, &probes, &m, &Override::default()).unwrap();
    assert_eq!(backends(&p.candidates), ["opencl:GPUOpenCL", "cpu:-"]);
}

#[test]
fn os_detection_failure_still_uses_backend_probes() {
    let a = Err(DetectError("CreateDXGIFactory1 0x887a0004".into()));
    let builds = [build("vulkan-x64", Backend::Vulkan), build("cpu-x64", Backend::Cpu)];
    let probes = vec![(
        "vulkan-x64".to_string(),
        Ok(vec![dev("Vulkan0", "AMD Radeon RX 6600", 8176, 7900)]),
    )];
    let p = plan(&a, &builds, &probes, &model(), &Override::default()).unwrap();
    assert_eq!(backends(&p.candidates), ["vulkan:Vulkan0", "cpu:-"]);
    assert!(p.notes[0].contains("GPU enumeration failed"));
}

#[test]
fn every_probe_failing_leaves_cpu() {
    let builds = [
        build("cuda-12.4-x64", Backend::Cuda),
        build("vulkan-x64", Backend::Vulkan),
        build("cpu-x64", Backend::Cpu),
    ];
    let probes = vec![
        ("cuda-12.4-x64".to_string(), Err(ProbeError::Spawn("NotFound".into()))),
        ("vulkan-x64".to_string(), Err(ProbeError::Timeout)),
    ];
    let p = plan(
        &Err(DetectError("x".into())),
        &builds,
        &probes,
        &model(),
        &Override::default(),
    )
    .unwrap();
    assert_eq!(backends(&p.candidates), ["cpu:-"]);
    assert_eq!(p.notes.len(), 3);
}

struct ScriptedLauncher {
    fail_first: usize,
    tried: Vec<String>,
}

impl Launcher for ScriptedLauncher {
    type Running = String;
    fn launch(&mut self, c: &Candidate) -> Result<String, String> {
        self.tried.push(c.label());
        if self.tried.len() <= self.fail_first {
            Err("health check failed: ErrorOutOfDeviceMemory".into())
        } else {
            Ok(c.label())
        }
    }
}

#[test]
fn gpu_initialisation_failure_falls_back_to_cpu() {
    let a = Ok(vec![adapter(0, "NVIDIA GeForce RTX 4050 Laptop GPU", NV, 6141)]);
    let builds = [
        build("cuda-12.4-x64", Backend::Cuda),
        build("vulkan-x64", Backend::Vulkan),
        build("cpu-x64", Backend::Cpu),
    ];
    let probes = vec![
        (
            "cuda-12.4-x64".to_string(),
            Ok(vec![dev("CUDA0", "NVIDIA GeForce RTX 4050 Laptop GPU", 6141, 5800)]),
        ),
        (
            "vulkan-x64".to_string(),
            Ok(vec![dev("Vulkan0", "NVIDIA GeForce RTX 4050 Laptop GPU", 6141, 5800)]),
        ),
    ];
    let p = plan(&a, &builds, &probes, &model(), &Override::default()).unwrap();
    let mut l = ScriptedLauncher {
        fail_first: 2,
        tried: vec![],
    };
    let started = launch_with_fallback(&p, &mut l).unwrap();
    assert_eq!(started.candidate.backend, Backend::Cpu);
    assert_eq!(started.failed_attempts.len(), 2);
    assert!(started.failed_attempts[0].candidate.starts_with("cuda on CUDA0"));
    assert!(started.failed_attempts[0].error.contains("OutOfDeviceMemory"));

    let mut l = ScriptedLauncher {
        fail_first: 9,
        tried: vec![],
    };
    assert_eq!(launch_with_fallback(&p, &mut l).err().unwrap().len(), 3);
}

#[test]
fn unsupported_model_backend_combination_is_skipped_unless_forced() {
    let a = Ok(vec![adapter(0, "AMD Radeon RX 6600", AMD, 8176)]);
    let builds = [build("vulkan-x64", Backend::Vulkan), build("cpu-x64", Backend::Cpu)];
    let probes = vec![(
        "vulkan-x64".to_string(),
        Ok(vec![dev("Vulkan0", "AMD Radeon RX 6600", 8176, 7900)]),
    )];
    let mut m = model();
    m.tensor_types.insert(GgmlType::IQ2XXS);

    let p = plan(&a, &builds, &probes, &m, &Override::default()).unwrap();
    assert_eq!(backends(&p.candidates), ["cpu:-"]);
    assert!(
        p.notes
            .iter()
            .any(|n| n.contains("IQ2XXS") && n.contains("unverified on vulkan")),
        "{:?}",
        p.notes
    );

    let forced = Override {
        backend: Some(Backend::Vulkan),
        ..Override::default()
    };
    let p = plan(&a, &builds, &probes, &m, &forced).unwrap();
    assert_eq!(backends(&p.candidates), ["vulkan:Vulkan0", "cpu:-"]);
    assert!(p.notes.iter().any(|n| n.contains("used because it was forced")));
}

#[test]
fn multiple_gpus_prefer_discrete_then_free_memory() {
    let a = Ok(vec![
        adapter(0, "Intel(R) UHD Graphics 770", INTEL, 128),
        adapter(1, "NVIDIA GeForce RTX 3060", NV, 12288),
        adapter(2, "NVIDIA GeForce RTX 4090", NV, 24564),
    ]);
    let builds = [
        build("cuda-12.4-x64", Backend::Cuda),
        build("vulkan-x64", Backend::Vulkan),
        build("cpu-x64", Backend::Cpu),
    ];
    let probes = vec![
        (
            "cuda-12.4-x64".to_string(),
            Ok(vec![
                dev("CUDA0", "NVIDIA GeForce RTX 4090", 24564, 23000),
                dev("CUDA1", "NVIDIA GeForce RTX 3060", 12288, 11800),
            ]),
        ),
        (
            "vulkan-x64".to_string(),
            Ok(vec![
                dev("Vulkan0", "Intel(R) UHD Graphics 770", 16000, 15000),
                dev("Vulkan1", "NVIDIA GeForce RTX 4090", 24564, 23000),
                dev("Vulkan2", "NVIDIA GeForce RTX 3060", 12288, 11800),
            ]),
        ),
    ];
    let p = plan(&a, &builds, &probes, &model(), &Override::default()).unwrap();
    assert_eq!(
        backends(&p.candidates),
        [
            "cuda:CUDA0",
            "cuda:CUDA1",
            "vulkan:Vulkan1",
            "vulkan:Vulkan2",
            "vulkan:Vulkan0",
            "cpu:-"
        ],
        "discrete before the integrated GPU even though it reports more shared memory"
    );

    // Override by device name picks the 3060 on its preferred backend first.
    let o = Override {
        device: Some(DeviceSelector::Name("3060".into())),
        ..Override::default()
    };
    let p = plan(&a, &builds, &probes, &model(), &o).unwrap();
    assert_eq!(backends(&p.candidates), ["cuda:CUDA1", "vulkan:Vulkan2", "cpu:-"]);

    // Override by backend + index.
    let o = Override::parse(Some("vulkan"), Some("0"), None, true).unwrap();
    let p = plan(&a, &builds, &probes, &model(), &o).unwrap();
    assert_eq!(backends(&p.candidates), ["vulkan:Vulkan0", "cpu:-"]);
}

#[test]
fn software_adapters_are_never_used() {
    let mut warp = adapter(0, "Microsoft Basic Render Driver", 0x1414, 0);
    warp.software = true;
    let builds = [build("vulkan-x64", Backend::Vulkan), build("cpu-x64", Backend::Cpu)];
    let probes = vec![(
        "vulkan-x64".to_string(),
        Ok(vec![dev("Vulkan0", "llvmpipe (LLVM 17.0.6, 256 bits)", 32000, 30000)]),
    )];
    let p = plan(&Ok(vec![warp]), &builds, &probes, &model(), &Override::default()).unwrap();
    assert_eq!(backends(&p.candidates), ["cpu:-"]);
}

#[test]
fn memory_fit_partial_offload_and_vision_on_cpu() {
    let a = Ok(vec![adapter(0, "NVIDIA GeForce GTX 1650", NV, 4096)]);
    let builds = [build("cuda-12.4-x64", Backend::Cuda), build("cpu-x64", Backend::Cpu)];
    let one = |free| {
        vec![(
            "cuda-12.4-x64".to_string(),
            Ok(vec![dev("CUDA0", "NVIDIA GeForce GTX 1650", 4096, free)]),
        )]
    };

    // 4000 MiB free: everything but the 800 MiB vision encoder fits.
    let p = plan(&a, &builds, &one(4000), &model(), &Override::default()).unwrap();
    assert_eq!(p.candidates[0].gpu_layers, GpuLayers::All);
    assert!(!p.candidates[0].mmproj_offload);
    assert!(
        p.candidates[0]
            .llama_args()
            .contains(&"--no-mmproj-offload".to_string())
    );

    // 2500 MiB free: partial offload of (2500-600-768)/69 = 16 of 36 layers.
    let p = plan(&a, &builds, &one(2500), &model(), &Override::default()).unwrap();
    assert_eq!(p.candidates[0].gpu_layers, GpuLayers::Count(16));
    assert_eq!(p.candidates[0].llama_args()[..4], ["--device", "CUDA0", "-ngl", "16"]);

    // 1500 MiB free: not worth it.
    let p = plan(&a, &builds, &one(1500), &model(), &Override::default()).unwrap();
    assert_eq!(backends(&p.candidates), ["cpu:-"]);
    assert!(p.notes.iter().any(|n| n.contains("insufficient memory")));
}

#[test]
fn strict_override_that_cannot_be_met_is_an_error() {
    let builds = [build("vulkan-x64", Backend::Vulkan), build("cpu-x64", Backend::Cpu)];
    let probes = vec![(
        "vulkan-x64".to_string(),
        Ok(vec![dev("Vulkan0", "AMD Radeon RX 6600", 8176, 7900)]),
    )];
    let strict = Override::parse(Some("cuda"), None, None, false).unwrap();
    assert!(matches!(
        plan(&Ok(vec![]), &builds, &probes, &model(), &strict),
        Err(SelectError::OverrideUnavailable(_))
    ));

    let lenient = Override::parse(Some("cuda"), None, None, true).unwrap();
    let p = plan(&Ok(vec![]), &builds, &probes, &model(), &lenient).unwrap();
    assert_eq!(backends(&p.candidates), ["cpu:-"]);
    assert!(
        p.notes
            .iter()
            .any(|n| n.contains("override backend cuda has no usable device"))
    );

    let cpu = Override::parse(Some("cpu"), None, None, false).unwrap();
    let p = plan(&Ok(vec![]), &builds, &probes, &model(), &cpu).unwrap();
    assert_eq!(backends(&p.candidates), ["cpu:-"]);
    assert_eq!(p.candidates[0].variant, "cpu-x64");
}

#[test]
fn override_parsing() {
    assert_eq!(
        Override::parse(Some("auto"), None, None, true).unwrap(),
        Override::default()
    );
    assert_eq!(
        Override::parse(Some("HIP"), Some("ROCm1"), Some("20"), true).unwrap(),
        Override {
            backend: Some(Backend::Rocm),
            device: Some(DeviceSelector::Id("ROCm1".into())),
            gpu_layers: Some(20),
            allow_fallback: true
        }
    );
    assert!(matches!(
        Override::parse(Some("directml"), None, None, true),
        Err(SelectError::UnknownBackend(_))
    ));
    assert!(matches!(
        Override::parse(None, None, Some("lots"), true),
        Err(SelectError::BadGpuLayers)
    ));
    assert_eq!(
        Override::parse(None, Some("rtx 4050"), None, true).unwrap().device,
        Some(DeviceSelector::Name("rtx 4050".into()))
    );
}

#[test]
fn no_build_installed_is_reported() {
    assert_eq!(
        plan(&Ok(vec![]), &[], &[], &model(), &Override::default()).err(),
        Some(SelectError::NoBuildInstalled)
    );
}
