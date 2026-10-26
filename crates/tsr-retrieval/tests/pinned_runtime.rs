//! Stage 0 checks for the pinned ONNX Runtime loader (plan D7, §10.1).
//!
//! The tests that load the real DLL are `#[ignore]`d because they need
//! `tools/fetch-sidecar.ps1 -Component onnxruntime -Variant win-x64` first. Run them with
//! `cargo test -p tsr-retrieval -- --ignored`. Each lives in its own test binary process
//! only by convention; `ort` allows a single environment per process, so keep exactly one
//! loading test in this file.

use std::path::PathBuf;

use tsr_retrieval::runtime::{RuntimeError, load_pinned, verify};

const PINNED_SHA256: &str = "0c42dc4e278f0968fdf9ebf1b1e54a74c657a9942e70803e453c91993451cf4c";

fn pinned_dll() -> PathBuf {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../third_party/onnxruntime/1.31.0/win-x64/onnxruntime-win-x64-1.31.0/lib/onnxruntime.dll")
        .canonicalize()
        .expect("pinned onnxruntime.dll missing: run tools/fetch-sidecar.ps1 -Component onnxruntime -Variant win-x64");
    // canonicalize() returns a `\\?\` verbatim path on Windows; use the ordinary form,
    // which is what the app will pass in.
    let s = p.to_string_lossy();
    PathBuf::from(s.strip_prefix(r"\\?\").unwrap_or(&s))
}

#[test]
fn relative_path_is_rejected() {
    let err = verify(std::path::Path::new("onnxruntime.dll"), PINNED_SHA256).unwrap_err();
    assert!(matches!(err, RuntimeError::NotAbsolute));
}

#[test]
fn tampered_file_is_rejected_before_loading() {
    let dir = tempfile::tempdir().unwrap();
    let fake = dir.path().join("onnxruntime.dll");
    std::fs::write(&fake, b"not the pinned runtime").unwrap();
    let err = load_pinned(&fake, PINNED_SHA256).err().expect("must fail");
    assert!(matches!(err, RuntimeError::HashMismatch { .. }), "{err:?}");
}

#[test]
fn malformed_expected_hash_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("x.dll");
    std::fs::write(&f, b"x").unwrap();
    assert!(matches!(verify(&f, "ABC").unwrap_err(), RuntimeError::BadExpectedHash));
    assert!(matches!(
        verify(&f, &PINNED_SHA256.to_uppercase()).unwrap_err(),
        RuntimeError::BadExpectedHash
    ));
}

#[test]
#[ignore = "needs the pinned onnxruntime.dll from tools/fetch-sidecar.ps1"]
fn pinned_runtime_loads_from_exact_path() {
    let dll = pinned_dll();
    let rt = load_pinned(&dll, PINNED_SHA256).expect("pinned runtime should load");
    assert_eq!(rt.sha256, PINNED_SHA256);

    #[cfg(windows)]
    {
        // The Windows-bundled copy must not have been mapped as a side effect.
        use std::os::windows::ffi::OsStrExt;
        use windows::Win32::System::LibraryLoader::GetModuleHandleW;
        use windows::core::PCWSTR;
        let sys = std::env::var("SystemRoot").unwrap() + r"\System32\onnxruntime.dll";
        let wide: Vec<u16> = std::ffi::OsStr::new(&sys).encode_wide().chain(Some(0)).collect();
        let loaded = unsafe { GetModuleHandleW(PCWSTR(wide.as_ptr())) };
        assert!(loaded.is_err(), "System32 onnxruntime.dll is mapped into the process");
    }
}
