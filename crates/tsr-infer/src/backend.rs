//! Compute backends, pinned llama.cpp builds and the real backend probe (plan §7.1).
//!
//! Each llama.cpp release ships one build per backend (CPU, CUDA, Vulkan, ROCm, SYCL, OpenVINO,
//! Metal, OpenCL). The executable is identical across builds; the backend is a loadable
//! `ggml-<backend>` library next to it. A build is only *installed* for Tessara if every file in
//! its directory matches the manifest in `tools/sidecar.lock` and no unlisted file exists — the
//! lock is compiled into the binary, so a swapped DLL cannot pass.
//!
//! Whether a backend works on this machine is decided by running the build's
//! `llama-server --list-devices` (no model needed): that initialises the backend for real and
//! reports each usable device with its memory. Nothing is downloaded here, ever.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::gguf::GgmlType;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum Backend {
    Cuda,
    Rocm,
    Vulkan,
    Sycl,
    OpenVino,
    Metal,
    OpenCl,
    Cpu,
}

impl Backend {
    pub const ALL: [Backend; 8] = [
        Backend::Cuda,
        Backend::Rocm,
        Backend::Vulkan,
        Backend::Sycl,
        Backend::OpenVino,
        Backend::Metal,
        Backend::OpenCl,
        Backend::Cpu,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Backend::Cuda => "cuda",
            Backend::Rocm => "rocm",
            Backend::Vulkan => "vulkan",
            Backend::Sycl => "sycl",
            Backend::OpenVino => "openvino",
            Backend::Metal => "metal",
            Backend::OpenCl => "opencl",
            Backend::Cpu => "cpu",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Backend::ALL
            .into_iter()
            .find(|b| b.as_str().eq_ignore_ascii_case(s.trim()))
            .or(match s.trim() {
                "hip" | "HIP" => Some(Backend::Rocm),
                "mps" | "MPS" => Some(Backend::Metal),
                _ => None,
            })
    }
}

/// How well a backend handles a tensor type. ggml's scheduler runs operations a backend does not
/// implement on the CPU instead, so "unsupported" costs speed, not correctness; `Unverified`
/// means we have no evidence either way and refuse to pick it automatically.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Support {
    Yes,
    Unverified,
}

/// Conservative support table for llama.cpp b11900 (documented in docs/hardware-backends.md).
/// CPU, CUDA, ROCm (HIP port of CUDA) and Metal are llama.cpp's reference backends for every
/// quantisation. The others are trusted for F32/F16/BF16, legacy and k-quants only.
pub fn tensor_support(backend: Backend, t: GgmlType) -> Support {
    use GgmlType::*;
    let common = matches!(
        t,
        F32 | F16 | Q4_0 | Q4_1 | Q5_0 | Q5_1 | Q8_0 | Q8_1 | Q2K | Q3K | Q4K | Q5K | Q6K | Q8K | I8 | I16 | I32
    );
    match backend {
        Backend::Cpu | Backend::Cuda | Backend::Rocm | Backend::Metal => {
            if matches!(t, Unknown(_)) {
                Support::Unverified
            } else {
                Support::Yes
            }
        }
        Backend::Vulkan | Backend::Sycl => {
            if common || (backend == Backend::Vulkan && t == BF16) {
                Support::Yes
            } else {
                Support::Unverified
            }
        }
        // OpenVINO and Adreno OpenCL backends are newer; only plain types and Q4_0/Q8_0 are trusted.
        Backend::OpenVino | Backend::OpenCl => {
            if matches!(t, F32 | F16 | Q4_0 | Q8_0) {
                Support::Yes
            } else {
                Support::Unverified
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Pinned builds (tools/sidecar.lock)

#[derive(Debug, Clone, Deserialize)]
struct Lock {
    llama_cpp: LockComponent,
}

#[derive(Debug, Clone, Deserialize)]
struct LockComponent {
    tag: String,
    assets: Vec<LockAsset>,
}

#[derive(Debug, Clone, Deserialize)]
struct LockAsset {
    variant: String,
    #[serde(default)]
    os: String,
    #[serde(default)]
    arch: String,
    #[serde(default)]
    backend: String,
    #[serde(default)]
    extract_into: Option<String>,
    #[serde(default)]
    files: Option<BTreeMap<String, String>>,
}

/// The lock file as compiled into this binary.
pub const SIDECAR_LOCK: &str = include_str!("../../../tools/sidecar.lock");

pub fn current_os() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

pub fn current_arch() -> &'static str {
    if cfg!(target_arch = "aarch64") { "arm64" } else { "x64" }
}

/// A build that exists on disk and matched its manifest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct InstalledBuild {
    pub variant: String,
    pub backend: Backend,
    pub dir: PathBuf,
    pub server_exe: PathBuf,
}

/// Why a pinned build is not usable. Content-free (paths are ours, not the user's).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct BuildNote {
    pub variant: String,
    pub reason: String,
}

fn sha256_file(p: &Path) -> std::io::Result<String> {
    let mut f = std::fs::File::open(p)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

fn list_files(dir: &Path, base: &Path, out: &mut Vec<String>) -> std::io::Result<()> {
    for e in std::fs::read_dir(dir)? {
        let e = e?;
        let p = e.path();
        if e.file_type()?.is_dir() {
            list_files(&p, base, out)?;
        } else {
            out.push(p.strip_prefix(base).unwrap_or(&p).to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}

/// Compare a directory with its manifest. Empty result = intact.
pub fn verify_build_dir(dir: &Path, manifest: &BTreeMap<String, String>) -> Vec<String> {
    let mut actual = Vec::new();
    if let Err(e) = list_files(dir, dir, &mut actual) {
        return vec![format!("unreadable: {}", e.kind())];
    }
    let mut problems = Vec::new();
    for f in &actual {
        match manifest.get(f) {
            None => problems.push(format!("unexpected file {f}")),
            Some(want) => match sha256_file(&dir.join(f)) {
                Ok(got) if &got == want => {}
                Ok(_) => problems.push(format!("hash mismatch {f}")),
                Err(e) => problems.push(format!("unreadable {f}: {}", e.kind())),
            },
        }
    }
    for f in manifest.keys() {
        if !actual.contains(f) {
            problems.push(format!("missing {f}"));
        }
    }
    problems
}

/// Find the pinned builds for this OS/arch under `third_party/llama_cpp/<tag>/` and keep only
/// those whose files match the manifest exactly.
pub fn installed_builds(third_party: &Path) -> (Vec<InstalledBuild>, Vec<BuildNote>) {
    installed_builds_from(SIDECAR_LOCK, third_party, current_os(), current_arch())
}

pub fn installed_builds_from(
    lock_json: &str,
    third_party: &Path,
    os: &str,
    arch: &str,
) -> (Vec<InstalledBuild>, Vec<BuildNote>) {
    let mut builds = Vec::new();
    let mut notes = Vec::new();
    let lock: Lock = match serde_json::from_str(lock_json) {
        Ok(l) => l,
        Err(_) => {
            notes.push(BuildNote {
                variant: "*".into(),
                reason: "sidecar.lock unreadable".into(),
            });
            return (builds, notes);
        }
    };
    let root = third_party.join("llama_cpp").join(&lock.llama_cpp.tag);
    let exe = if os == "windows" {
        "llama-server.exe"
    } else {
        "llama-server"
    };
    for a in &lock.llama_cpp.assets {
        if a.extract_into.is_some() || a.os != os || a.arch != arch {
            continue;
        }
        let Some(backend) = Backend::parse(&a.backend) else {
            continue;
        };
        let dir = root.join(&a.variant);
        if !dir.exists() {
            continue; // not installed: normal, not a problem
        }
        let Some(manifest) = &a.files else {
            notes.push(BuildNote {
                variant: a.variant.clone(),
                reason: "no recorded manifest; not trusted".into(),
            });
            continue;
        };
        let problems = verify_build_dir(&dir, manifest);
        if !problems.is_empty() {
            tracing::warn!(variant = %a.variant, problems = problems.len(), first = %problems[0], "llama.cpp build failed integrity check");
            notes.push(BuildNote {
                variant: a.variant.clone(),
                reason: format!("integrity: {}", problems[0]),
            });
            continue;
        }
        // Builds unpack either flat or into one sub-folder; find the server executable.
        let server_exe = manifest
            .keys()
            .find(|k| k.rsplit('/').next() == Some(exe))
            .map(|k| dir.join(k))
            .unwrap_or_else(|| dir.join(exe));
        builds.push(InstalledBuild {
            variant: a.variant.clone(),
            backend,
            dir,
            server_exe,
        });
    }
    (builds, notes)
}

// ---------------------------------------------------------------------------------------------
// Backend probe

/// A device as a backend reports it (`Vulkan0: AMD Radeon (TM) Graphics (8096 MiB, 7691 MiB free)`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct BackendDevice {
    /// What `--device` takes, e.g. `Vulkan0`, `CUDA1`.
    pub id: String,
    pub name: String,
    pub total_mib: Option<u64>,
    pub free_mib: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error, Serialize)]
pub enum ProbeError {
    #[error("could not start: {0}")]
    Spawn(String),
    #[error("timed out")]
    Timeout,
    #[error("exited with code {0:?}")]
    Exit(Option<i32>),
    #[error("unrecognised output")]
    Unparseable,
}

/// Parse `llama-server --list-devices` output.
pub fn parse_list_devices(out: &str) -> Result<Vec<BackendDevice>, ProbeError> {
    let mut lines = out
        .lines()
        .skip_while(|l| !l.trim_start().starts_with("Available devices:"));
    if lines.next().is_none() {
        return Err(ProbeError::Unparseable);
    }
    let mut devices = Vec::new();
    for raw in lines {
        // Device lines are indented; the first non-indented line is other log output.
        if !raw.starts_with([' ', '\t']) {
            break;
        }
        let l = raw.trim();
        if l.is_empty() || l == "(none)" {
            continue;
        }
        let Some((id, rest)) = l.split_once(": ") else { break };
        if id.contains(' ') {
            break;
        }
        let (name, mem) = match rest.rfind(" (") {
            Some(i) if rest.ends_with(')') => (&rest[..i], Some(&rest[i + 2..rest.len() - 1])),
            _ => (rest, None),
        };
        let (mut total, mut free) = (None, None);
        if let Some(m) = mem {
            for part in m.split(',') {
                let num = part.split_whitespace().next().and_then(|n| n.parse::<u64>().ok());
                if part.contains("free") {
                    free = num;
                } else {
                    total = num;
                }
            }
        }
        devices.push(BackendDevice {
            id: id.to_owned(),
            name: name.trim().to_owned(),
            total_mib: total,
            free_mib: free,
        });
    }
    Ok(devices)
}

pub trait BackendProbe {
    fn list_devices(&self, build: &InstalledBuild) -> Result<Vec<BackendDevice>, ProbeError>;
}

/// Environment variables a child process needs to start; everything else is dropped so that
/// `LLAMA_ARG_*`, `HF_TOKEN`, `GGML_*`, `CUDA_VISIBLE_DEVICES` and similar cannot change what the
/// sidecar does (Stage 0 finding, plan §7.1).
pub const SIDECAR_ENV_ALLOWLIST: &[&str] = &[
    "SystemRoot",
    "SystemDrive",
    "windir",
    "TEMP",
    "TMP",
    "NUMBER_OF_PROCESSORS",
    "PROCESSOR_ARCHITECTURE",
    "HOME",
];

/// Runs `llama-server --list-devices` from a verified build.
pub struct LlamaServerProbe {
    pub timeout: Duration,
}

impl Default for LlamaServerProbe {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(20),
        }
    }
}

impl BackendProbe for LlamaServerProbe {
    fn list_devices(&self, build: &InstalledBuild) -> Result<Vec<BackendDevice>, ProbeError> {
        let mut cmd = Command::new(&build.server_exe);
        cmd.arg("--list-devices")
            .env_clear()
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        for k in SIDECAR_ENV_ALLOWLIST {
            if let Some(v) = std::env::var_os(k) {
                cmd.env(k, v);
            }
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(windows::Win32::System::Threading::CREATE_NO_WINDOW.0);
        }
        let started = Instant::now();
        let mut child = cmd.spawn().map_err(|e| ProbeError::Spawn(format!("{:?}", e.kind())))?;
        let mut stdout = child.stdout.take().expect("piped");
        let reader = std::thread::spawn(move || {
            let mut s = String::new();
            let _ = stdout.read_to_string(&mut s);
            s
        });
        let status = loop {
            if let Some(st) = child
                .try_wait()
                .map_err(|e| ProbeError::Spawn(format!("{:?}", e.kind())))?
            {
                break st;
            }
            if started.elapsed() > self.timeout {
                let _ = child.kill();
                let _ = child.wait();
                return Err(ProbeError::Timeout);
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let out = reader.join().unwrap_or_default();
        if !status.success() {
            return Err(ProbeError::Exit(status.code()));
        }
        let devices = parse_list_devices(&out)?;
        tracing::info!(variant = %build.variant, backend = build.backend.as_str(), devices = devices.len(),
                       ms = started.elapsed().as_millis() as u64, "backend probe");
        Ok(devices)
    }
}

/// Fixed probe results per variant for tests (SIMULATED backends).
pub struct MockProbe(pub BTreeMap<String, Result<Vec<BackendDevice>, ProbeError>>);

impl BackendProbe for MockProbe {
    fn list_devices(&self, build: &InstalledBuild) -> Result<Vec<BackendDevice>, ProbeError> {
        self.0
            .get(&build.variant)
            .cloned()
            .unwrap_or(Err(ProbeError::Spawn("NotFound".into())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_real_list_devices_output() {
        // Captured from the pinned b11900 builds on the dev machine (REAL output text).
        let vulkan = "Available devices:\n  Vulkan0: AMD Radeon (TM) Graphics (8096 MiB, 7691 MiB free)\n";
        let d = parse_list_devices(vulkan).unwrap();
        assert_eq!(
            d,
            vec![BackendDevice {
                id: "Vulkan0".into(),
                name: "AMD Radeon (TM) Graphics".into(),
                total_mib: Some(8096),
                free_mib: Some(7691)
            }]
        );
        assert_eq!(parse_list_devices("Available devices:\n  (none)\n").unwrap(), vec![]);
        assert!(matches!(
            parse_list_devices("error: unknown argument"),
            Err(ProbeError::Unparseable)
        ));
    }

    #[test]
    fn parses_multi_gpu_output_with_log_noise() {
        // SIMULATED multi-GPU text in the same format.
        let out = "ggml_cuda_init: found 2 CUDA devices:\nAvailable devices:\n  CUDA0: NVIDIA GeForce RTX 4090 (24564 MiB, 23000 MiB free)\n  CUDA1: NVIDIA GeForce RTX 3060 (12288 MiB, 11800 MiB free)\nmain: exiting\n";
        let d = parse_list_devices(out).unwrap();
        assert_eq!(d.len(), 2);
        assert_eq!(d[1].id, "CUDA1");
        assert_eq!(d[1].free_mib, Some(11800));
    }

    #[test]
    fn backend_names() {
        assert_eq!(Backend::parse("HIP"), Some(Backend::Rocm));
        assert_eq!(Backend::parse("mps"), Some(Backend::Metal));
        assert_eq!(Backend::parse(" Vulkan "), Some(Backend::Vulkan));
        assert_eq!(Backend::parse("directml"), None);
    }

    #[test]
    fn support_table() {
        assert_eq!(tensor_support(Backend::Vulkan, GgmlType::Q4K), Support::Yes);
        assert_eq!(tensor_support(Backend::Vulkan, GgmlType::IQ2XXS), Support::Unverified);
        assert_eq!(tensor_support(Backend::Cuda, GgmlType::IQ2XXS), Support::Yes);
        assert_eq!(tensor_support(Backend::OpenCl, GgmlType::Q4K), Support::Unverified);
        assert_eq!(tensor_support(Backend::Cpu, GgmlType::Unknown(99)), Support::Unverified);
    }

    #[test]
    fn lock_parses_and_integrity_rejects_tampering() {
        let d = tempfile::tempdir().unwrap();
        let lock = r#"{"llama_cpp":{"tag":"bX","assets":[
            {"variant":"vk","os":"windows","arch":"x64","backend":"vulkan","files":{"llama-server.exe":"HASH","ggml-vulkan.dll":"HASH2"}},
            {"variant":"cpu","os":"windows","arch":"x64","backend":"cpu","files":null},
            {"variant":"mac","os":"macos","arch":"arm64","backend":"metal","files":{}}]}}"#;
        let dir = d.path().join("llama_cpp/bX/vk");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("llama-server.exe"), b"exe").unwrap();
        std::fs::write(dir.join("ggml-vulkan.dll"), b"dll").unwrap();
        std::fs::create_dir_all(d.path().join("llama_cpp/bX/cpu")).unwrap();
        let hash = |b: &[u8]| Sha256::digest(b).iter().map(|x| format!("{x:02x}")).collect::<String>();
        let good = lock
            .replace("\"HASH\"", &format!("\"{}\"", hash(b"exe")))
            .replace("\"HASH2\"", &format!("\"{}\"", hash(b"dll")));

        let (b, notes) = installed_builds_from(&good, d.path(), "windows", "x64");
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].backend, Backend::Vulkan);
        assert_eq!(
            notes,
            vec![BuildNote {
                variant: "cpu".into(),
                reason: "no recorded manifest; not trusted".into()
            }]
        );

        // A planted DLL makes the whole build untrusted.
        std::fs::write(dir.join("ggml-cuda.dll"), b"evil").unwrap();
        let (b, notes) = installed_builds_from(&good, d.path(), "windows", "x64");
        assert!(b.is_empty());
        assert!(notes[0].reason.contains("unexpected file ggml-cuda.dll"));
    }

    #[test]
    fn compiled_in_lock_is_valid() {
        let l: Lock = serde_json::from_str(SIDECAR_LOCK).unwrap();
        assert!(l.llama_cpp.assets.iter().any(|a| a.backend == "vulkan"));
        for a in &l.llama_cpp.assets {
            assert!(Backend::parse(&a.backend).is_some(), "{}", a.variant);
        }
    }
}
