//! Loading the pinned ONNX Runtime (plan D7, §10.1).
//!
//! `ort` is built with `load-dynamic`, so it links to nothing at build time and downloads
//! nothing. At run time it needs to be told which DLL to load. If nobody tells it, the first
//! session falls back to the `ORT_DYLIB_PATH` environment variable or a bare
//! `onnxruntime.dll` resolved through the normal Windows DLL search. Windows 11 ships its own
//! `System32\onnxruntime.dll`, so that fallback could load an unpinned copy.
//!
//! [`load_pinned`] closes that gap: it checks the SHA-256 of an absolute path, hands exactly
//! that file to `ort`, and returns the `Environment` that every embedder must be built with
//! (`fastembed::InitOptionsUserDefined::with_environment`). Call it once, before anything
//! else touches `ort`.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("ONNX Runtime path must be absolute")]
    NotAbsolute,
    #[error("ONNX Runtime file could not be read")]
    Unreadable(#[source] std::io::Error),
    #[error("ONNX Runtime hash mismatch")]
    HashMismatch { expected: String, actual: String },
    #[error("expected hash is not 64 lowercase hex characters")]
    BadExpectedHash,
    #[error("ONNX Runtime could not be loaded")]
    Load(String),
    #[error("ONNX Runtime environment could not be created")]
    Environment(String),
    #[error("the loaded onnxruntime.dll is not the pinned file")]
    WrongModuleLoaded,
}

/// A verified, loaded ONNX Runtime.
pub struct PinnedRuntime {
    pub path: PathBuf,
    pub sha256: String,
    pub environment: ort::environment::Environment,
}

/// SHA-256 of a file as lowercase hex.
pub fn sha256_file(path: &Path) -> Result<String, RuntimeError> {
    let mut file = File::open(path).map_err(RuntimeError::Unreadable)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = file.read(&mut buf).map_err(RuntimeError::Unreadable)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Check the file against the pinned hash without loading it.
pub fn verify(path: &Path, expected_sha256: &str) -> Result<String, RuntimeError> {
    if !path.is_absolute() {
        return Err(RuntimeError::NotAbsolute);
    }
    let well_formed = expected_sha256.len() == 64
        && expected_sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    if !well_formed {
        return Err(RuntimeError::BadExpectedHash);
    }
    let actual = sha256_file(path)?;
    if actual != expected_sha256 {
        return Err(RuntimeError::HashMismatch {
            expected: expected_sha256.to_owned(),
            actual,
        });
    }
    Ok(actual)
}

/// Verify, load and initialise the pinned ONNX Runtime.
///
/// Limitation: there is a window between hashing and loading in which the file could be
/// swapped. Closing it needs write access to the install directory, i.e. malware running as
/// the same user, which the threat model (§15.2) places out of scope.
pub fn load_pinned(path: &Path, expected_sha256: &str) -> Result<PinnedRuntime, RuntimeError> {
    let sha256 = verify(path, expected_sha256)?;

    let environment = ort::init_from(path)
        .map_err(|e| RuntimeError::Load(e.to_string()))?
        .with_name("tessara")
        .build()
        .map_err(|e| RuntimeError::Environment(e.to_string()))?;

    #[cfg(windows)]
    ensure_loaded_module_is(path)?;

    Ok(PinnedRuntime {
        path: path.to_path_buf(),
        sha256,
        environment,
    })
}

/// Confirm Windows mapped the pinned file (by full path) into this process.
#[cfg(windows)]
fn ensure_loaded_module_is(path: &Path) -> Result<(), RuntimeError> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::core::PCWSTR;

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: `wide` is a NUL-terminated UTF-16 string that outlives the call.
    unsafe { GetModuleHandleW(PCWSTR(wide.as_ptr())) }
        .map(|_| ())
        .map_err(|_| RuntimeError::WrongModuleLoaded)
}
