//! Model manifest, safe file paths and digest checks (plan §7.1, §10.1, onboarding "locate
//! existing files").
//!
//! `tools/models.lock` is compiled in and is the only trust anchor: a model file is usable only if
//! its size and digest match it. A digest computed from a downloaded or user-supplied file proves
//! nothing on its own — it is compared against the manifest.
//!
//! [`inspect_dir`] checks a folder the user picked: it lists only that folder (and the manifest's
//! own sub-paths inside it, e.g. `onnx/`), never searches elsewhere, and never copies, moves or
//! deletes anything. Every rejected file gets a reason the UI can show.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::LazyLock;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use sha1::Sha1;
use sha2::{Digest as _, Sha256};

use crate::cancel::CancelToken;
use crate::gguf::read_gguf;

pub const MODELS_LOCK: &str = include_str!("../../../tools/models.lock");

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Llm,
    Mmproj,
    Embedder,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelFile {
    /// Relative path inside the model folder (and inside the repo), `/`-separated.
    pub path: String,
    pub size: u64,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub git_blob_sha1: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelEntry {
    pub id: String,
    pub role: Role,
    pub repo: String,
    pub revision: String,
    pub license: String,
    #[serde(default)]
    pub dims: Option<u32>,
    pub files: Vec<ModelFile>,
}

impl ModelEntry {
    /// Download URL for one file at the pinned revision (Hugging Face `resolve`).
    pub fn url(&self, f: &ModelFile) -> String {
        format!(
            "https://huggingface.co/{}/resolve/{}/{}",
            self.repo, self.revision, f.path
        )
    }

    pub fn total_size(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }
}

#[derive(Debug, Deserialize)]
struct Lock {
    models: Vec<ModelEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error, Serialize)]
pub enum ManifestError {
    #[error("manifest does not parse: {0}")]
    Parse(String),
    #[error("manifest entry {0} is invalid: {1}")]
    Invalid(String, &'static str),
}

/// Parse and validate a manifest (exactly one digest per file, safe paths, hex digests).
pub fn parse_manifest(json: &str) -> Result<Vec<ModelEntry>, ManifestError> {
    let lock: Lock = serde_json::from_str(json).map_err(|e| ManifestError::Parse(e.to_string()))?;
    for m in &lock.models {
        let bad = |why| Err(ManifestError::Invalid(m.id.clone(), why));
        if m.files.is_empty() {
            return bad("no files");
        }
        if !m.revision.chars().all(|c| c.is_ascii_hexdigit()) || m.revision.len() != 40 {
            return bad("revision must be a full commit hash");
        }
        for f in &m.files {
            if safe_relative_path(&f.path).is_err() {
                return bad("unsafe file path");
            }
            match (&f.sha256, &f.git_blob_sha1) {
                (Some(h), None) if is_hex(h, 64) => {}
                (None, Some(h)) if is_hex(h, 40) => {}
                _ => return bad("each file needs exactly one valid digest"),
            }
        }
    }
    Ok(lock.models)
}

fn is_hex(s: &str, len: usize) -> bool {
    s.len() == len && s.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
}

pub static MANIFEST: LazyLock<Vec<ModelEntry>> =
    LazyLock::new(|| parse_manifest(MODELS_LOCK).expect("tools/models.lock is valid"));

pub fn entry(id: &str) -> Option<&'static ModelEntry> {
    MANIFEST.iter().find(|m| m.id == id)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error, Serialize)]
pub enum PathError {
    #[error("empty path")]
    Empty,
    #[error("absolute paths, drive letters and UNC prefixes are not allowed")]
    Absolute,
    #[error("`..` is not allowed")]
    ParentDir,
    #[error("component is not a safe file name")]
    BadComponent,
}

const RESERVED: &[&str] = &[
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8", "com9", "lpt1", "lpt2",
    "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// Validate a manifest path and turn it into a relative `PathBuf`. Rejects traversal, absolute and
/// drive paths, alternate data streams (`:`), reserved device names and trailing dots/spaces.
pub fn safe_relative_path(p: &str) -> Result<PathBuf, PathError> {
    if p.is_empty() {
        return Err(PathError::Empty);
    }
    if p.starts_with(['/', '\\']) || p.contains(':') {
        return Err(PathError::Absolute);
    }
    let mut out = PathBuf::new();
    for part in p.split(['/', '\\']) {
        if part == ".." {
            return Err(PathError::ParentDir);
        }
        let stem = part.split('.').next().unwrap_or("").to_ascii_lowercase();
        if part.is_empty()
            || part == "."
            || part.ends_with(['.', ' '])
            || RESERVED.contains(&stem.as_str())
            || part.chars().any(|c| c.is_control() || "<>\"|?*".contains(c))
        {
            return Err(PathError::BadComponent);
        }
        out.push(part);
    }
    // Belt and braces: the platform parser must agree that this is a plain relative path.
    if out.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(PathError::BadComponent);
    }
    Ok(out)
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error, Serialize)]
pub enum VerifyError {
    #[error("file missing")]
    Missing,
    #[error("not readable: {0}")]
    Unreadable(String),
    #[error("size {actual} bytes, expected {expected}")]
    WrongSize { actual: u64, expected: u64 },
    #[error("contents do not match the published digest")]
    DigestMismatch,
    #[error("cancelled")]
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct VerifyProgress {
    pub done: u64,
    pub total: u64,
}

/// Check size and digest of `path` against `f`. Reads the file once, in 1 MiB blocks; reports
/// progress and stops promptly when cancelled.
pub fn verify_file(
    path: &Path,
    f: &ModelFile,
    cancel: &CancelToken,
    progress: &mut dyn FnMut(VerifyProgress),
) -> Result<(), VerifyError> {
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(VerifyError::Missing),
        Err(e) => return Err(VerifyError::Unreadable(format!("{:?}", e.kind()))),
    };
    if !meta.is_file() {
        return Err(VerifyError::Unreadable("not a file".into()));
    }
    if meta.len() != f.size {
        return Err(VerifyError::WrongSize {
            actual: meta.len(),
            expected: f.size,
        });
    }
    let mut file = std::fs::File::open(path).map_err(|e| VerifyError::Unreadable(format!("{:?}", e.kind())))?;
    let mut sha256 = f.sha256.as_ref().map(|_| Sha256::new());
    let mut sha1 = f.git_blob_sha1.as_ref().map(|_| {
        let mut h = Sha1::new();
        h.update(format!("blob {}\0", f.size).as_bytes());
        h
    });
    let mut buf = vec![0u8; 1 << 20];
    let mut done = 0u64;
    let mut last_report = Instant::now();
    loop {
        if cancel.is_cancelled() {
            return Err(VerifyError::Cancelled);
        }
        let n = file
            .read(&mut buf)
            .map_err(|e| VerifyError::Unreadable(format!("{:?}", e.kind())))?;
        if n == 0 {
            break;
        }
        if let Some(h) = sha256.as_mut() {
            h.update(&buf[..n]);
        }
        if let Some(h) = sha1.as_mut() {
            h.update(&buf[..n]);
        }
        done += n as u64;
        if last_report.elapsed().as_millis() >= 100 {
            progress(VerifyProgress { done, total: f.size });
            last_report = Instant::now();
        }
    }
    progress(VerifyProgress { done, total: f.size });
    if done != f.size {
        return Err(VerifyError::WrongSize {
            actual: done,
            expected: f.size,
        });
    }
    let ok = match (sha256, sha1) {
        (Some(h), _) => hex(&h.finalize()) == *f.sha256.as_ref().expect("set"),
        (None, Some(h)) => hex(&h.finalize()) == *f.git_blob_sha1.as_ref().expect("set"),
        (None, None) => false,
    };
    if ok { Ok(()) } else { Err(VerifyError::DigestMismatch) }
}

pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// What a file in the chosen folder is, from its own header.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum FileKind {
    Gguf { architecture: Option<String>, tensors: u64 },
    Onnx,
    Json,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum FileStatus {
    /// Matches the manifest (size; digest too if it was checked).
    Ok {
        digest_checked: bool,
    },
    /// Size matches; digest not checked yet (fast scan).
    SizeOk,
    Rejected(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FileReport {
    pub path: String,
    pub size: Option<u64>,
    pub kind: Option<FileKind>,
    pub status: FileStatus,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EntryReport {
    pub id: String,
    pub role: Role,
    pub files: Vec<FileReport>,
    /// Every file present and accepted.
    pub usable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DirReport {
    pub dir_readable: bool,
    pub problem: Option<String>,
    pub entries: Vec<EntryReport>,
    /// Files in the folder that are not in the manifest (shown so the user knows they were seen
    /// and why they are not used). Names only.
    pub unknown_files: Vec<String>,
}

fn sniff(path: &Path) -> Option<FileKind> {
    let mut magic = [0u8; 8];
    let mut f = std::fs::File::open(path).ok()?;
    let n = f.read(&mut magic).ok()?;
    if n >= 4 && &magic[..4] == b"GGUF" {
        return Some(match read_gguf(path) {
            Ok(g) => FileKind::Gguf {
                architecture: g.architecture.clone(),
                tensors: g.tensor_count,
            },
            Err(_) => FileKind::Gguf {
                architecture: None,
                tensors: 0,
            },
        });
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    Some(match ext.as_str() {
        "onnx" => FileKind::Onnx,
        "json" => FileKind::Json,
        _ => FileKind::Other,
    })
}

/// Check a folder the user chose for the manifest's files. With `check_digests`, every present
/// file of the right size is hashed (slow for multi-GB files; cancellable).
pub fn inspect_dir(
    dir: &Path,
    manifest: &[ModelEntry],
    check_digests: bool,
    cancel: &CancelToken,
    progress: &mut dyn FnMut(&str, VerifyProgress),
) -> DirReport {
    let listing = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(e) => {
            return DirReport {
                dir_readable: false,
                problem: Some(format!("folder cannot be read: {:?}", e.kind())),
                entries: Vec::new(),
                unknown_files: Vec::new(),
            };
        }
    };
    let known: BTreeMap<String, ()> = manifest
        .iter()
        .flat_map(|m| {
            m.files
                .iter()
                .map(|f| (f.path.split('/').next().unwrap_or("").to_owned(), ()))
        })
        .collect();
    let mut unknown_files: Vec<String> = listing
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !known.contains_key(n))
        .collect();
    unknown_files.sort();

    let mut entries = Vec::new();
    for m in manifest {
        let mut files = Vec::new();
        for f in &m.files {
            let rel = safe_relative_path(&f.path).expect("manifest validated");
            let p = dir.join(&rel);
            let size = std::fs::metadata(&p).ok().filter(|m| m.is_file()).map(|m| m.len());
            let kind = size.and_then(|_| sniff(&p));
            let status = match size {
                None => FileStatus::Rejected("missing".into()),
                Some(s) if s < f.size => FileStatus::Rejected(format!(
                    "incomplete: {s} of {} bytes (an interrupted download?)",
                    f.size
                )),
                Some(s) if s != f.size => FileStatus::Rejected(format!("wrong size: {s} bytes, expected {}", f.size)),
                Some(_) => {
                    let header_ok = match (&m.role, &kind) {
                        (Role::Llm | Role::Mmproj, Some(FileKind::Gguf { tensors, .. })) => *tensors > 0,
                        (Role::Llm | Role::Mmproj, _) => false,
                        _ => true,
                    };
                    if !header_ok {
                        FileStatus::Rejected("not a readable GGUF model file".into())
                    } else if check_digests {
                        match verify_file(&p, f, cancel, &mut |pr| progress(&f.path, pr)) {
                            Ok(()) => FileStatus::Ok { digest_checked: true },
                            Err(VerifyError::Cancelled) => FileStatus::SizeOk,
                            Err(e) => FileStatus::Rejected(e.to_string()),
                        }
                    } else {
                        FileStatus::SizeOk
                    }
                }
            };
            files.push(FileReport {
                path: f.path.clone(),
                size,
                kind,
                status,
            });
        }
        let usable = files.iter().all(|f| matches!(f.status, FileStatus::Ok { .. }));
        entries.push(EntryReport {
            id: m.id.clone(),
            role: m.role,
            files,
            usable,
        });
    }
    DirReport {
        dir_readable: true,
        problem: None,
        entries,
        unknown_files,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_manifest_is_valid() {
        let m = &*MANIFEST;
        assert!(m.iter().any(|e| e.role == Role::Llm));
        assert!(m.iter().any(|e| e.role == Role::Mmproj));
        assert!(m.iter().any(|e| e.role == Role::Embedder && e.dims == Some(384)));
        let llm = entry("qwen3-vl-4b-instruct-q4_k_m").unwrap();
        assert_eq!(
            llm.url(&llm.files[0]),
            "https://huggingface.co/Qwen/Qwen3-VL-4B-Instruct-GGUF/resolve/1cd86afb9a95c410a6038ab3b40d8b578c892266/Qwen3VL-4B-Instruct-Q4_K_M.gguf"
        );
    }

    #[test]
    fn manifest_validation_rejects_bad_entries() {
        let base = |files: &str, rev: &str| {
            format!(
                r#"{{"models":[{{"id":"x","role":"llm","repo":"a/b","revision":"{rev}","license":"MIT","files":[{files}]}}]}}"#
            )
        };
        let rev = "0".repeat(40);
        let good =
            r#"{"path":"m.gguf","size":1,"sha256":"0000000000000000000000000000000000000000000000000000000000000000"}"#;
        assert!(parse_manifest(&base(good, &rev)).is_ok());
        assert!(
            parse_manifest(&base(good, "main")).is_err(),
            "branch names are not pins"
        );
        for bad in [
            r#"{"path":"../m.gguf","size":1,"sha256":"0000000000000000000000000000000000000000000000000000000000000000"}"#,
            r#"{"path":"m.gguf","size":1}"#,
            r#"{"path":"m.gguf","size":1,"sha256":"abc"}"#,
            r#"{"path":"m.gguf","size":1,"sha256":"0000000000000000000000000000000000000000000000000000000000000000","git_blob_sha1":"0000000000000000000000000000000000000000"}"#,
            r#"{"path":"m.gguf","size":1,"sha256":"0000000000000000000000000000000000000000000000000000000000000000","extra":1}"#,
        ] {
            assert!(parse_manifest(&base(bad, &rev)).is_err(), "{bad}");
        }
    }

    #[test]
    fn unsafe_paths_are_rejected() {
        for ok in ["model.gguf", "onnx/model.onnx", "a/b/c.json"] {
            assert!(safe_relative_path(ok).is_ok(), "{ok}");
        }
        for bad in [
            "",
            "../x",
            "a/../../x",
            "/etc/passwd",
            "\\\\server\\share\\x",
            "C:\\Windows\\x",
            "c:x",
            "a/./b",
            "file.gguf:stream",
            "CON",
            "nul.txt",
            "a/LPT1.gguf",
            "trailing.",
            "trailing ",
            "a//b",
            "q?.gguf",
        ] {
            assert!(safe_relative_path(bad).is_err(), "{bad:?}");
        }
    }

    fn file_with(content: &[u8], sha256: bool) -> ModelFile {
        let size = content.len() as u64;
        if sha256 {
            ModelFile {
                path: "f.bin".into(),
                size,
                sha256: Some(hex(&Sha256::digest(content))),
                git_blob_sha1: None,
            }
        } else {
            let mut h = Sha1::new();
            h.update(format!("blob {size}\0").as_bytes());
            h.update(content);
            ModelFile {
                path: "f.bin".into(),
                size,
                sha256: None,
                git_blob_sha1: Some(hex(&h.finalize())),
            }
        }
    }

    #[test]
    fn verify_checks_size_and_both_digest_kinds() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("f.bin");
        std::fs::write(&p, b"hello model").unwrap();
        let c = CancelToken::new();
        for sha in [true, false] {
            let f = file_with(b"hello model", sha);
            verify_file(&p, &f, &c, &mut |_| {}).unwrap();
            let mut wrong = f.clone();
            wrong.size += 1;
            assert!(matches!(
                verify_file(&p, &wrong, &c, &mut |_| {}),
                Err(VerifyError::WrongSize { .. })
            ));
        }
        std::fs::write(&p, b"hello modeX").unwrap();
        assert_eq!(
            verify_file(&p, &file_with(b"hello model", true), &c, &mut |_| {}),
            Err(VerifyError::DigestMismatch)
        );
        assert_eq!(
            verify_file(&d.path().join("nope"), &file_with(b"x", true), &c, &mut |_| {}),
            Err(VerifyError::Missing)
        );
        // Known git blob id: `printf 'hello\n' | git hash-object --stdin`.
        let f = ModelFile {
            path: "h".into(),
            size: 6,
            sha256: None,
            git_blob_sha1: Some("ce013625030ba8dba906f756967f9e9ca394464a".into()),
        };
        std::fs::write(&p, b"hello\n").unwrap();
        verify_file(&p, &f, &c, &mut |_| {}).unwrap();
    }

    #[test]
    fn inspect_explains_missing_incomplete_corrupt_and_unknown_files() {
        let d = tempfile::tempdir().unwrap();
        let good_gguf = d.path().join("good.gguf");
        crate::gguf::write_test_gguf(&good_gguf, "qwen3vl", &[("qwen3vl.block_count", 2)], &[12, 12]).unwrap();
        let bytes = std::fs::read(&good_gguf).unwrap();
        let entry = |id: &str, role, files: Vec<ModelFile>| ModelEntry {
            id: id.into(),
            role,
            repo: "a/b".into(),
            revision: "0".repeat(40),
            license: "MIT".into(),
            dims: None,
            files,
        };
        let mut llm = file_with(&bytes, true);
        llm.path = "good.gguf".into();
        let mut partial = file_with(&[0u8; 100], true);
        partial.path = "partial.gguf".into();
        std::fs::write(d.path().join("partial.gguf"), [0u8; 40]).unwrap();
        let mut fake = file_with(b"not a gguf at all", true);
        fake.path = "fake.gguf".into();
        std::fs::write(d.path().join("fake.gguf"), b"not a gguf at all").unwrap();
        let mut missing = file_with(b"x", true);
        missing.path = "onnx/model.onnx".into();
        std::fs::write(d.path().join("notes.txt"), b"user file").unwrap();
        let manifest = vec![
            entry("ok", Role::Llm, vec![llm]),
            entry("partial", Role::Llm, vec![partial]),
            entry("fake", Role::Mmproj, vec![fake]),
            entry("emb", Role::Embedder, vec![missing]),
        ];
        let before: Vec<_> = std::fs::read_dir(d.path())
            .unwrap()
            .flatten()
            .map(|e| e.file_name())
            .collect();
        let r = inspect_dir(d.path(), &manifest, true, &CancelToken::new(), &mut |_, _| {});
        let status = |id: &str| r.entries.iter().find(|e| e.id == id).unwrap().files[0].status.clone();
        assert_eq!(status("ok"), FileStatus::Ok { digest_checked: true });
        assert!(matches!(status("partial"), FileStatus::Rejected(s) if s.starts_with("incomplete")));
        assert!(matches!(status("fake"), FileStatus::Rejected(s) if s.contains("GGUF")));
        assert_eq!(status("emb"), FileStatus::Rejected("missing".into()));
        assert_eq!(r.entries.iter().filter(|e| e.usable).count(), 1);
        assert_eq!(r.unknown_files, vec!["notes.txt"]);
        let after: Vec<_> = std::fs::read_dir(d.path())
            .unwrap()
            .flatten()
            .map(|e| e.file_name())
            .collect();
        assert_eq!(before, after, "inspection never changes the folder");
        let none = inspect_dir(
            &d.path().join("absent"),
            &manifest,
            false,
            &CancelToken::new(),
            &mut |_, _| {},
        );
        assert!(!none.dir_readable && none.problem.is_some());
    }

    #[test]
    fn verification_can_be_cancelled() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("f.bin");
        std::fs::write(&p, vec![7u8; 3 << 20]).unwrap();
        let f = file_with(&vec![7u8; 3 << 20], true);
        let c = CancelToken::new();
        c.cancel(crate::cancel::CancelReason::Cancelled);
        assert_eq!(verify_file(&p, &f, &c, &mut |_| {}), Err(VerifyError::Cancelled));
    }
}

/// Digests verified once are recorded here and trusted while each file's size and modification
/// time are unchanged (start-up cannot hash 3 GB every time). Any change re-verifies. Same-user
/// tampering that also restores the timestamp is out of scope (plan §15.2).
pub const STAMP: &str = ".tessara-verified.json";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
struct StampFile {
    path: String,
    size: u64,
    mtime_ns: u128,
}

fn stamp_of(dir: &Path, r: &DirReport) -> Vec<StampFile> {
    let mut v: Vec<StampFile> = r
        .entries
        .iter()
        .flat_map(|e| e.files.iter())
        .filter(|f| matches!(f.status, FileStatus::SizeOk | FileStatus::Ok { .. }))
        .filter_map(|f| {
            let m = std::fs::metadata(dir.join(&f.path)).ok()?;
            let mtime_ns = m
                .modified()
                .ok()?
                .duration_since(std::time::UNIX_EPOCH)
                .ok()?
                .as_nanos();
            Some(StampFile {
                path: f.path.clone(),
                size: m.len(),
                mtime_ns,
            })
        })
        .collect();
    v.sort_by(|a, b| a.path.cmp(&b.path));
    v
}

/// Like [`inspect_dir`] with digests, but reuses the stamp when nothing changed since the last
/// full check; files then report `Ok { digest_checked: true }`.
pub fn inspect_dir_stamped(dir: &Path, manifest: &[ModelEntry], cancel: &CancelToken) -> DirReport {
    let mut quick = inspect_dir(dir, manifest, false, cancel, &mut |_, _| {});
    quick.unknown_files.retain(|f| f != STAMP);
    let now = stamp_of(dir, &quick);
    let recorded: Option<Vec<StampFile>> = std::fs::read(dir.join(STAMP))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok());
    if !now.is_empty() && recorded.as_ref() == Some(&now) {
        for f in quick.entries.iter_mut().flat_map(|e| e.files.iter_mut()) {
            if f.status == FileStatus::SizeOk {
                f.status = FileStatus::Ok { digest_checked: true };
            }
        }
        return quick;
    }
    let mut full = inspect_dir(dir, manifest, true, cancel, &mut |_, _| {});
    full.unknown_files.retain(|f| f != STAMP);
    let verified = stamp_of(dir, &full);
    if !verified.is_empty()
        && let Ok(b) = serde_json::to_vec(&verified)
    {
        let _ = std::fs::write(dir.join(STAMP), b);
    }
    full
}

#[cfg(test)]
mod stamp_tests {
    use super::*;

    #[test]
    fn digests_are_rechecked_after_any_change_and_trusted_otherwise() {
        let d = tempfile::tempdir().unwrap();
        let body = b"synthetic tokenizer {}";
        std::fs::write(d.path().join("tokenizer.json"), body).unwrap();
        let digest: String = Sha256::digest(body).iter().map(|b| format!("{b:02x}")).collect();
        let lock = format!(
            r#"{{"models":[{{"id":"t","role":"embedder","repo":"a/b","revision":"{}","license":"MIT","files":[{{"path":"tokenizer.json","size":{},"sha256":"{digest}"}}]}}]}}"#,
            "0".repeat(40),
            body.len()
        );
        let m = parse_manifest(&lock).unwrap();
        let ok = |r: &DirReport| {
            r.entries[0]
                .files
                .iter()
                .all(|f| f.status == FileStatus::Ok { digest_checked: true })
        };
        let c = CancelToken::new();
        assert!(ok(&inspect_dir_stamped(d.path(), &m, &c)), "first use: full check");
        assert!(d.path().join(STAMP).exists());
        assert!(
            ok(&inspect_dir_stamped(d.path(), &m, &c)),
            "unchanged: trusted via stamp"
        );
        // Same size, different bytes, newer mtime: must be re-hashed and rejected.
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(d.path().join("tokenizer.json"), b"synthetic tokenizer {!").unwrap();
        let r = inspect_dir_stamped(d.path(), &m, &c);
        assert!(matches!(r.entries[0].files[0].status, FileStatus::Rejected(_)), "{r:?}");
        assert!(
            r.unknown_files.is_empty(),
            "the stamp is not reported as an unknown file"
        );
    }
}
