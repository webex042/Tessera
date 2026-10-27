//! The sidecar's environment is the allow-list only (plan §7.1; Stage 2 exit criterion).
//! Own test binary because it changes this process's environment.
//!
//! - SIMULATED: the fake server reports the variable *names* it received.
//! - REAL (ignored by default; needs the pinned llama.cpp CPU build under `third_party/`, no
//!   model): llama-server in router mode honours `LLAMA_ARG_LOG_FILE` when it inherits the
//!   environment (control run), and does not when Tessara's sidecar code starts it.
//!
//! Run the real part with `cargo test -p tsr-infer --test sidecar_env -- --ignored --test-threads=1`.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use tsr_infer::cancel::CancelToken;
use tsr_infer::sidecar::{SidecarSpec, start};

const POISON: &[(&str, &str)] = &[
    ("LLAMA_ARG_AGENT", "1"),
    ("LLAMA_ARG_HF_REPO", "example/never-download"),
    ("LLAMA_ARG_MCP_SERVERS_JSON", "{}"),
    ("HF_TOKEN", "hf_fake"),
    ("HTTPS_PROXY", "http://127.0.0.1:9"),
    ("GGML_VK_VISIBLE_DEVICES", "7"),
    ("CUDA_VISIBLE_DEVICES", "7"),
];

fn poison_env() {
    for (k, v) in POISON {
        // SAFETY: tests in this binary run with --test-threads=1 semantics for env: each test sets
        // the same values, and nothing reads the environment concurrently in a way that matters.
        unsafe { std::env::set_var(k, v) };
    }
}

#[test]
fn fake_sidecar_sees_only_the_allow_list() {
    poison_env();
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("env.txt");
    let spec = SidecarSpec {
        server_exe: PathBuf::from(env!("CARGO_BIN_EXE_tsr-fake-llama")),
        model: None,
        mmproj: None,
        device_args: vec![],
        ctx: 2048,
        threads: None,
        extra_args: vec!["--fake-env-file".into(), f.display().to_string()],
        extra_env: vec![],
        ready_timeout: Duration::from_secs(10),
    };
    let p = start(&spec, &CancelToken::new()).unwrap();
    let names = std::fs::read_to_string(&f).unwrap();
    drop(p);
    let names: Vec<&str> = names.lines().collect();
    println!("SIMULATED child environment names: {names:?}");
    for n in &names {
        assert!(
            tsr_infer::backend::SIDECAR_ENV_ALLOWLIST
                .iter()
                .any(|a| a.eq_ignore_ascii_case(n)),
            "unexpected variable {n}"
        );
    }
    for (k, _) in POISON {
        assert!(!names.iter().any(|n| n.eq_ignore_ascii_case(k)), "{k} leaked");
    }
}

fn real_server() -> Option<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../third_party");
    let (builds, _) = tsr_infer::backend::installed_builds(&root);
    builds
        .into_iter()
        .find(|b| b.backend == tsr_infer::backend::Backend::Cpu)
        .map(|b| b.server_exe)
}

/// Router mode needs a model directory and a cache directory; both are empty temp dirs, so no
/// model is loaded and (with `--offline`) nothing is downloaded.
fn router_args(models: &Path) -> Vec<String> {
    vec![
        "--models-dir".into(),
        models.display().to_string(),
        "--no-models-autoload".into(),
    ]
}

#[test]
#[ignore = "REAL: needs the pinned llama.cpp CPU build in third_party/ (no model)"]
fn real_llama_server_ignores_inherited_llama_arg_variables() {
    let Some(exe) = real_server() else {
        eprintln!("SKIPPED: pinned CPU build not installed (python tools/fetch_sidecar.py llama_cpp cpu-x64)");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let models = dir.path().join("models");
    let cache = dir.path().join("cache");
    std::fs::create_dir_all(&models).unwrap();
    std::fs::create_dir_all(&cache).unwrap();
    let control_log = dir.path().join("control.log");
    let ours_log = dir.path().join("ours.log");

    // Control: the same server with an inherited environment writes the log file the variable
    // names — proving the variable is live on this build.
    let port = tsr_infer::sidecar::free_loopback_port().unwrap();
    let mut control = std::process::Command::new(&exe)
        .args([
            "--host",
            "127.0.0.1",
            "--port",
            &port.to_string(),
            "--api-key",
            "k",
            "--offline",
            "--no-webui",
        ])
        .args(router_args(&models))
        .env("LLAMA_CACHE", &cache)
        .env("LLAMA_ARG_LOG_FILE", &control_log)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_secs(3));
    let _ = control.kill();
    let _ = control.wait();
    assert!(
        control_log.exists(),
        "control run did not honour LLAMA_ARG_LOG_FILE; test cannot prove anything"
    );

    // Ours: the parent environment carries the variable (and worse ones); the sidecar must not.
    poison_env();
    // SAFETY: see poison_env.
    unsafe { std::env::set_var("LLAMA_ARG_LOG_FILE", &ours_log) };
    let spec = SidecarSpec {
        server_exe: exe,
        model: None,
        mmproj: None,
        device_args: vec![],
        ctx: 2048,
        threads: Some(1),
        extra_args: router_args(&models),
        // The one explicit addition router mode needs; production passes nothing here.
        extra_env: vec![("LLAMA_CACHE".into(), OsString::from(cache.as_os_str()))],
        ready_timeout: Duration::from_secs(30),
    };
    let p = start(&spec, &CancelToken::new()).expect("real llama-server starts in router mode");
    println!("REAL llama-server ready in {} ms (pid {})", p.ready_ms, p.pid());
    std::thread::sleep(Duration::from_secs(2));
    drop(p);
    // SAFETY: see poison_env.
    unsafe { std::env::remove_var("LLAMA_ARG_LOG_FILE") };
    assert!(!ours_log.exists(), "inherited LLAMA_ARG_LOG_FILE reached the sidecar");
    let leftovers: Vec<_> = std::fs::read_dir(&cache).unwrap().collect();
    assert!(leftovers.is_empty(), "router mode wrote to the cache (download?)");
}
