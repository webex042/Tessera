//! REAL hardware tests. They run the pinned llama.cpp builds under `third_party/` against the GPU
//! of the machine running the tests, so they are `#[ignore]`d by default:
//!
//!     python tools/fetch_sidecar.py llama_cpp cpu-x64
//!     python tools/fetch_sidecar.py llama_cpp vulkan-x64      # or the build for your GPU
//!     cargo test -p tsr-infer --test real_hardware -- --ignored --nocapture --test-threads=1
//!
//! Assertions are written to hold on any machine (with or without a GPU); the printed report is
//! the evidence of what this particular machine did.

use std::path::{Path, PathBuf};
use std::time::Duration;

use tsr_infer::backend::{Backend, BackendProbe, InstalledBuild, LlamaServerProbe, ProbeError, installed_builds};
use tsr_infer::hardware::SystemAdapters;
use tsr_infer::select::{ModelNeeds, Override, plan};

fn third_party() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../third_party")
}

fn model() -> ModelNeeds {
    ModelNeeds {
        weights_mib: 2500,
        mmproj_mib: 800,
        kv_mib: 600,
        n_layers: 36,
        tensor_types: [tsr_infer::gguf::GgmlType::Q4K, tsr_infer::gguf::GgmlType::Q6K].into(),
    }
}

#[test]
#[ignore = "needs pinned llama.cpp builds in third_party/"]
fn real_detection_and_selection_on_this_machine() {
    let report = tsr_infer::detect(&third_party(), &SystemAdapters, &LlamaServerProbe::default());
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
    assert!(
        !report.builds.is_empty(),
        "no verified build installed: {:?}",
        report.build_notes
    );
    assert!(
        report.build_notes.is_empty(),
        "integrity problems: {:?}",
        report.build_notes
    );

    let p = plan(
        &report.adapters_result(),
        &report.builds,
        &report.probes,
        &model(),
        &Override::default(),
    )
    .unwrap();
    println!(
        "PLAN: {:#?}",
        p.candidates
            .iter()
            .map(|c| (c.label(), c.reason.clone(), c.llama_args()))
            .collect::<Vec<_>>()
    );
    println!("NOTES: {:#?}", p.notes);
    assert_eq!(
        p.candidates.last().unwrap().backend,
        Backend::Cpu,
        "CPU is always the last resort"
    );
    let any_gpu = report
        .probes
        .iter()
        .any(|(_, r)| r.as_ref().is_ok_and(|d| !d.is_empty()));
    assert_eq!(
        p.candidates[0].backend != Backend::Cpu,
        any_gpu,
        "a GPU is chosen exactly when a backend found one"
    );
}

#[test]
#[ignore = "needs pinned llama.cpp builds in third_party/"]
fn real_cpu_only_path() {
    let (builds, _) = installed_builds(&third_party());
    let cpu: Vec<InstalledBuild> = builds.into_iter().filter(|b| b.backend == Backend::Cpu).collect();
    assert!(!cpu.is_empty(), "fetch the cpu build first");
    let p = plan(&Ok(vec![]), &cpu, &[], &model(), &Override::default()).unwrap();
    assert_eq!(p.candidates.len(), 1);
    assert_eq!(p.candidates[0].backend, Backend::Cpu);
}

#[test]
#[ignore = "spawns a process"]
fn real_probe_of_missing_executable_fails_cleanly() {
    let b = InstalledBuild {
        variant: "ghost".into(),
        backend: Backend::Vulkan,
        dir: PathBuf::from("does-not-exist"),
        server_exe: PathBuf::from("does-not-exist/llama-server.exe"),
    };
    let r = LlamaServerProbe {
        timeout: Duration::from_secs(5),
    }
    .list_devices(&b);
    assert!(matches!(r, Err(ProbeError::Spawn(_))), "{r:?}");
}

/// The probe must ignore the caller's environment. `GGML_VK_VISIBLE_DEVICES` hides Vulkan devices
/// from llama.cpp; first prove that it does, then prove the probe is unaffected by it.
#[test]
#[ignore = "needs the pinned Vulkan build and a Vulkan-capable GPU"]
fn real_probe_ignores_inherited_environment() {
    let (builds, _) = installed_builds(&third_party());
    let Some(vk) = builds.iter().find(|b| b.backend == Backend::Vulkan) else {
        panic!("fetch the vulkan build first");
    };
    let baseline = LlamaServerProbe::default().list_devices(vk).unwrap();
    assert!(!baseline.is_empty(), "this test needs a Vulkan device");

    let direct = std::process::Command::new(&vk.server_exe)
        .arg("--list-devices")
        .env("GGML_VK_VISIBLE_DEVICES", "99")
        .output()
        .unwrap();
    let hidden = tsr_infer::backend::parse_list_devices(&String::from_utf8_lossy(&direct.stdout)).unwrap();
    assert!(
        hidden.is_empty(),
        "the variable should hide devices when inherited: {hidden:?}"
    );

    // SAFETY: this test binary runs this test alone (--test-threads=1 in the documented command).
    unsafe { std::env::set_var("GGML_VK_VISIBLE_DEVICES", "99") };
    let probed = LlamaServerProbe::default().list_devices(vk);
    unsafe { std::env::remove_var("GGML_VK_VISIBLE_DEVICES") };
    assert_eq!(probed.unwrap(), baseline, "probe must not inherit GGML_* variables");
}
