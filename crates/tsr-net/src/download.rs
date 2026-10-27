//! User-approved, resumable, verified model downloads (plan §15.4; Stage 2 task 2).
//!
//! Guarantees:
//! - **Approval:** nothing is fetched without an [`Approval`] naming the model; the UI creates it
//!   only after showing the user the host, the files and the total size.
//! - **Trust:** the expected size and digest come from the compiled-in manifest
//!   (`tools/models.lock`), never from the server. A file is registered as usable only after the
//!   whole file matches.
//! - **Where:** files land only under the configured models folder, at manifest paths checked by
//!   [`tsr_infer::models::safe_relative_path`]. Data is written to `<name>.part` with a
//!   `<name>.part.json` sidecar describing what it is a part of; the final name appears only by
//!   rename after verification. An existing final file is never overwritten unless the caller
//!   set `replace_existing` (and even then only by a verified replacement).
//! - **Resume:** `Range: bytes=N-`. A `206` must carry `Content-Range: bytes N-…/size` with
//!   exactly our offset and the manifest size; anything else (a `200`, a wrong offset or total)
//!   discards the part and restarts from zero. A digest mismatch after a resumed download also
//!   restarts once from zero, since a bad resume is the likely cause.
//! - **Network:** HTTPS only, OS TLS stack and certificate store, no proxies, redirects followed
//!   by hand and only to Hugging Face hosts, per-read stall timeout, cancellable.

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tsr_infer::cancel::CancelToken;
use tsr_infer::models::{ModelEntry, ModelFile, PathError, VerifyError, safe_relative_path, verify_file};

/// Proof that the user approved downloading these models. Created by the onboarding UI after it
/// showed what will be fetched from where ([`plan_download`]).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Approval {
    model_ids: Vec<String>,
    pub approved_at_ms: i64,
}

impl Approval {
    /// Record the user's decision. Call this only from the user's own click in the UI.
    pub fn by_user(model_ids: &[&str], approved_at_ms: i64) -> Self {
        Approval {
            model_ids: model_ids.iter().map(|s| s.to_string()).collect(),
            approved_at_ms,
        }
    }

    pub fn covers(&self, id: &str) -> bool {
        self.model_ids.iter().any(|m| m == id)
    }
}

/// What the approval dialog shows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DownloadPlan {
    pub model_id: String,
    pub host: &'static str,
    pub files: Vec<(String, u64)>,
    pub total_bytes: u64,
    pub license: String,
}

pub fn plan_download(e: &ModelEntry) -> DownloadPlan {
    DownloadPlan {
        model_id: e.id.clone(),
        host: "huggingface.co",
        files: e.files.iter().map(|f| (f.path.clone(), f.size)).collect(),
        total_bytes: e.total_size(),
        license: e.license.clone(),
    }
}

#[derive(Clone, Debug)]
pub struct DownloadConfig {
    /// The models folder. Created if missing.
    pub dest_root: PathBuf,
    pub connect_timeout: Duration,
    /// Longest silence while reading the body.
    pub stall_timeout: Duration,
    pub max_redirects: u32,
    pub replace_existing: bool,
    /// Tests only: replace `https://huggingface.co` with this loopback base URL (plain HTTP is
    /// accepted for `127.0.0.1` only). `None` in the app.
    pub test_base_url: Option<String>,
}

impl DownloadConfig {
    pub fn new(dest_root: PathBuf) -> Self {
        DownloadConfig {
            dest_root,
            connect_timeout: Duration::from_secs(15),
            stall_timeout: Duration::from_secs(60),
            max_redirects: 5,
            replace_existing: false,
            test_base_url: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error, Serialize)]
pub enum DownloadError {
    #[error("the user has not approved downloading this model")]
    NotApproved,
    #[error("unsafe destination path: {0}")]
    UnsafePath(PathError),
    #[error("destination escapes the models folder")]
    OutsideRoot,
    #[error("refusing non-HTTPS URL")]
    Insecure,
    #[error("redirect to a host that is not allowed: {0}")]
    HostNotAllowed(String),
    #[error("too many redirects")]
    TooManyRedirects,
    #[error("server answered HTTP {0}")]
    Http(u16),
    #[error("network error: {0}")]
    Network(String),
    #[error("server sent more data than the published size")]
    TooLong,
    #[error("download kept failing to complete")]
    Incomplete,
    #[error("verification failed: {0}")]
    Verify(VerifyError),
    #[error("a different file with this name already exists")]
    ExistsDifferent,
    #[error("file system error: {0}")]
    Io(String),
    #[error("cancelled")]
    Cancelled,
}

fn io(e: std::io::Error) -> DownloadError {
    DownloadError::Io(format!("{:?}", e.kind()))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Progress<'a> {
    pub model_id: &'a str,
    pub file: &'a str,
    pub done: u64,
    pub total: u64,
    pub phase: Phase,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Phase {
    Downloading,
    Verifying,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum FileOutcome {
    /// Fetched and verified. `resumed_from` > 0 when an earlier part was reused.
    Downloaded {
        path: String,
        resumed_from: u64,
        restarts: u32,
    },
    /// Already there and verified; nothing fetched.
    AlreadyPresent { path: String },
}

#[derive(Serialize, Deserialize, PartialEq, Eq)]
struct PartMeta {
    url: String,
    size: u64,
    digest: String,
}

/// Hugging Face and its CDNs. Downloads start at huggingface.co and are redirected to these.
fn host_allowed(host: &str) -> bool {
    let h = host.to_ascii_lowercase();
    h == "huggingface.co" || h.ends_with(".huggingface.co") || h == "hf.co" || h.ends_with(".hf.co")
}

/// Split `scheme://host[:port]/path` without a URL library. `None` for anything else.
fn split_url(url: &str) -> Option<(&str, &str, &str)> {
    let (scheme, rest) = url.split_once("://")?;
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    if authority.is_empty() || authority.contains('@') {
        return None; // no credentials in URLs
    }
    let host = authority.rsplit_once(':').map_or(authority, |(h, _)| h);
    Some((scheme, host, path))
}

fn check_url(url: &str, cfg: &DownloadConfig) -> Result<(), DownloadError> {
    let (scheme, host, _) = split_url(url).ok_or(DownloadError::Insecure)?;
    match scheme {
        "https" if host_allowed(host) => Ok(()),
        "https" => Err(DownloadError::HostNotAllowed(host.to_owned())),
        "http" if cfg.test_base_url.is_some() && host == "127.0.0.1" => Ok(()),
        _ => Err(DownloadError::Insecure),
    }
}

fn agent(cfg: &DownloadConfig) -> ureq::Agent {
    use ureq::tls::{RootCerts, TlsConfig, TlsProvider};
    ureq::Agent::config_builder()
        .https_only(cfg.test_base_url.is_none())
        .max_redirects(0)
        .http_status_as_error(false)
        .proxy(None)
        .timeout_connect(Some(cfg.connect_timeout))
        .timeout_recv_response(Some(cfg.connect_timeout * 2))
        .timeout_per_read(Some(cfg.stall_timeout))
        .user_agent("Tessara-model-download")
        .tls_config(
            TlsConfig::builder()
                .provider(TlsProvider::NativeTls)
                .root_certs(RootCerts::PlatformVerifier)
                .build(),
        )
        .build()
        .into()
}

fn net(e: ureq::Error) -> DownloadError {
    match e {
        ureq::Error::Timeout(_) => DownloadError::Network("timeout".into()),
        ureq::Error::Io(e) => DownloadError::Network(format!("{:?}", e.kind())),
        ureq::Error::Tls(_) => DownloadError::Network("tls".into()),
        ureq::Error::HostNotFound => DownloadError::Network("host not found".into()),
        ureq::Error::ConnectionFailed => DownloadError::Network("connection failed".into()),
        ureq::Error::RequireHttpsOnly(_) => DownloadError::Insecure,
        _ => DownloadError::Network("request failed".into()),
    }
}

/// GET with optional range; follows redirects only to allowed hosts.
fn get(
    agent: &ureq::Agent,
    url: &str,
    offset: u64,
    cfg: &DownloadConfig,
) -> Result<ureq::http::Response<ureq::Body>, DownloadError> {
    let mut url = url.to_owned();
    for _ in 0..=cfg.max_redirects {
        check_url(&url, cfg)?;
        let mut req = agent.get(&url);
        if offset > 0 {
            req = req.header("Range", &format!("bytes={offset}-"));
        }
        let resp = req.call().map_err(net)?;
        let st = resp.status().as_u16();
        if (300..400).contains(&st) {
            let loc = resp
                .headers()
                .get("location")
                .and_then(|v| v.to_str().ok())
                .ok_or(DownloadError::Http(st))?;
            url = if loc.starts_with('/') {
                let (scheme, _, _) = split_url(&url).ok_or(DownloadError::Insecure)?;
                let rest = url.split_once("://").map(|(_, r)| r).unwrap_or("");
                let authority = rest.split('/').next().unwrap_or("");
                format!("{scheme}://{authority}{loc}")
            } else {
                loc.to_owned()
            };
            continue;
        }
        return Ok(resp);
    }
    Err(DownloadError::TooManyRedirects)
}

/// `Content-Range: bytes START-END/TOTAL` → (START, TOTAL).
fn content_range(v: &str) -> Option<(u64, u64)> {
    let v = v.trim().strip_prefix("bytes ")?;
    let (range, total) = v.split_once('/')?;
    let (start, _end) = range.split_once('-')?;
    Some((start.trim().parse().ok()?, total.trim().parse().ok()?))
}

fn digest_of(f: &ModelFile) -> String {
    f.sha256.clone().or_else(|| f.git_blob_sha1.clone()).unwrap_or_default()
}

fn with_suffix(p: &Path, suffix: &str) -> PathBuf {
    let mut s = p.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

/// Resolve the destination and make sure it stays inside the models folder.
fn destination(cfg: &DownloadConfig, f: &ModelFile) -> Result<PathBuf, DownloadError> {
    let rel = safe_relative_path(&f.path).map_err(DownloadError::UnsafePath)?;
    std::fs::create_dir_all(&cfg.dest_root).map_err(io)?;
    let root = cfg.dest_root.canonicalize().map_err(io)?;
    let dest = root.join(&rel);
    let parent = dest.parent().ok_or(DownloadError::OutsideRoot)?;
    std::fs::create_dir_all(parent).map_err(io)?;
    let parent = parent.canonicalize().map_err(io)?;
    if !parent.starts_with(&root) {
        return Err(DownloadError::OutsideRoot); // e.g. a junction inside the folder
    }
    Ok(parent.join(dest.file_name().ok_or(DownloadError::OutsideRoot)?))
}

/// Download one file of an approved model. Idempotent: a verified file already in place is kept.
pub fn download_file(
    e: &ModelEntry,
    f: &ModelFile,
    approval: &Approval,
    cfg: &DownloadConfig,
    cancel: &CancelToken,
    progress: &mut dyn FnMut(Progress<'_>),
) -> Result<FileOutcome, DownloadError> {
    if !approval.covers(&e.id) {
        return Err(DownloadError::NotApproved);
    }
    let dest = destination(cfg, f)?;
    let part = with_suffix(&dest, ".part");
    let meta_path = with_suffix(&dest, ".part.json");
    let mut url = e.url(f);
    if let Some(base) = &cfg.test_base_url {
        url = url.replacen("https://huggingface.co", base, 1);
    }
    check_url(&url, cfg)?;

    let mut report = |done, phase| {
        progress(Progress {
            model_id: &e.id,
            file: &f.path,
            done,
            total: f.size,
            phase,
        })
    };

    if dest.exists() {
        match verify_file(&dest, f, cancel, &mut |p| report(p.done, Phase::Verifying)) {
            Ok(()) => {
                return Ok(FileOutcome::AlreadyPresent { path: f.path.clone() });
            }
            Err(VerifyError::Cancelled) => return Err(DownloadError::Cancelled),
            Err(_) if !cfg.replace_existing => return Err(DownloadError::ExistsDifferent),
            Err(_) => {}
        }
    }

    // Reuse a part only if it is a part of exactly this file.
    let meta = PartMeta {
        url: e.url(f),
        size: f.size,
        digest: digest_of(f),
    };
    let meta_ok = std::fs::read(&meta_path)
        .ok()
        .and_then(|b| serde_json::from_slice::<PartMeta>(&b).ok())
        .is_some_and(|m| m == meta);
    if !meta_ok {
        let _ = std::fs::remove_file(&part);
    }
    std::fs::write(&meta_path, serde_json::to_vec(&meta).expect("json")).map_err(io)?;

    let agent = agent(cfg);
    let mut restarts = 0u32;
    let mut resumed_from = 0u64;
    let mut short_reads = 0u32;
    let mut buf = vec![0u8; 256 << 10];
    loop {
        if cancel.is_cancelled() {
            return Err(DownloadError::Cancelled);
        }
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&part)
            .map_err(io)?;
        let mut offset = file.metadata().map_err(io)?.len();
        if offset > f.size {
            file.set_len(0).map_err(io)?;
            offset = 0;
        }
        if offset > 0 && resumed_from == 0 && restarts == 0 {
            resumed_from = offset;
        }
        if offset < f.size {
            let resp = get(&agent, &url, offset, cfg)?;
            let st = resp.status().as_u16();
            let restart = |file: &mut File| -> Result<(), DownloadError> {
                file.set_len(0).map_err(io)?;
                file.seek(SeekFrom::Start(0)).map_err(io)?;
                Ok(())
            };
            match st {
                200 => {
                    if offset > 0 {
                        // Server ignored the range: start over from byte 0 with this response.
                        restart(&mut file)?;
                        offset = 0;
                        resumed_from = 0;
                    }
                }
                206 => {
                    let cr = resp
                        .headers()
                        .get("content-range")
                        .and_then(|v| v.to_str().ok())
                        .and_then(content_range);
                    if cr != Some((offset, f.size)) {
                        // Wrong piece or wrong total: never splice it in.
                        restart(&mut file)?;
                        restarts += 1;
                        resumed_from = 0;
                        if restarts > 3 {
                            return Err(DownloadError::Incomplete);
                        }
                        continue;
                    }
                }
                416 => {
                    // We asked beyond the end, yet the part is short: start over.
                    restart(&mut file)?;
                    restarts += 1;
                    if restarts > 3 {
                        return Err(DownloadError::Incomplete);
                    }
                    continue;
                }
                s => return Err(DownloadError::Http(s)),
            }
            file.seek(SeekFrom::Start(offset)).map_err(io)?;
            let mut body = resp.into_body();
            let mut reader = body.with_config().limit(f.size - offset + 1).reader();
            let mut done = offset;
            let mut last = Instant::now();
            loop {
                if cancel.is_cancelled() {
                    file.flush().map_err(io)?;
                    return Err(DownloadError::Cancelled); // the part stays for a later resume
                }
                let n = match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => n,
                    Err(_) => break, // connection dropped: resume below
                };
                if done + n as u64 > f.size {
                    restart(&mut file)?;
                    return Err(DownloadError::TooLong);
                }
                file.write_all(&buf[..n]).map_err(io)?;
                done += n as u64;
                if last.elapsed() >= Duration::from_millis(200) {
                    report(done, Phase::Downloading);
                    last = Instant::now();
                }
            }
            file.flush().map_err(io)?;
            file.sync_all().map_err(io)?;
            report(done, Phase::Downloading);
            if done < f.size {
                short_reads += 1;
                if short_reads > 5 {
                    return Err(DownloadError::Incomplete);
                }
                continue; // resume from where it stopped
            }
        }
        drop(file);
        match verify_file(&part, f, cancel, &mut |p| report(p.done, Phase::Verifying)) {
            Ok(()) => {
                if dest.exists() && cfg.replace_existing {
                    std::fs::remove_file(&dest).map_err(io)?;
                }
                std::fs::rename(&part, &dest).map_err(io)?;
                let _ = std::fs::remove_file(&meta_path);
                tracing::info!(bytes = f.size, status = "verified", "model file downloaded");
                return Ok(FileOutcome::Downloaded {
                    path: f.path.clone(),
                    resumed_from,
                    restarts,
                });
            }
            Err(VerifyError::Cancelled) => return Err(DownloadError::Cancelled),
            Err(err) => {
                // Most likely a bad resume or a corrupted transfer: one full retry from zero.
                let _ = std::fs::remove_file(&part);
                if restarts >= 1 {
                    let _ = std::fs::remove_file(&meta_path);
                    return Err(DownloadError::Verify(err));
                }
                restarts += 1;
                resumed_from = 0;
            }
        }
    }
}

/// Download every file of an approved model.
pub fn download_model(
    e: &ModelEntry,
    approval: &Approval,
    cfg: &DownloadConfig,
    cancel: &CancelToken,
    progress: &mut dyn FnMut(Progress<'_>),
) -> Result<Vec<FileOutcome>, DownloadError> {
    if !approval.covers(&e.id) {
        return Err(DownloadError::NotApproved);
    }
    e.files
        .iter()
        .map(|f| download_file(e, f, approval, cfg, cancel, progress))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_hugging_face_hosts_over_https() {
        let cfg = DownloadConfig::new(PathBuf::from("x"));
        for ok in [
            "https://huggingface.co/a/b/resolve/x/f",
            "https://cdn-lfs.hf.co/x",
            "https://cas-bridge.xethub.hf.co/x?sig=1",
            "https://cdn-lfs-us-1.huggingface.co/x",
        ] {
            assert!(check_url(ok, &cfg).is_ok(), "{ok}");
        }
        for (bad, why) in [
            ("http://huggingface.co/x", "plain http"),
            ("https://evil.example/x", "other host"),
            ("https://huggingface.co.evil.example/x", "suffix trick"),
            ("https://evilhf.co/x", "suffix without dot"),
            ("https://user:pw@huggingface.co/x", "credentials"),
            ("ftp://huggingface.co/x", "scheme"),
            ("http://127.0.0.1:8080/x", "loopback without test config"),
        ] {
            assert!(check_url(bad, &cfg).is_err(), "{why}: {bad}");
        }
    }

    #[test]
    fn content_range_parsing() {
        assert_eq!(content_range("bytes 100-199/1000"), Some((100, 1000)));
        assert_eq!(content_range("bytes 0-0/1"), Some((0, 1)));
        assert_eq!(content_range("bytes */1000"), None);
        assert_eq!(content_range("items 1-2/3"), None);
    }

    #[test]
    fn approval_is_per_model() {
        let a = Approval::by_user(&["m1"], 0);
        assert!(a.covers("m1") && !a.covers("m2"));
    }
}
