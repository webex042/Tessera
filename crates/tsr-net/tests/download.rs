//! Download behaviour against a SIMULATED loopback HTTP server (no internet): resume, servers that
//! ignore or botch ranges, dropped connections, corrupt and oversized bodies, redirects, approval,
//! idempotency and path safety.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use sha2::{Digest, Sha256};
use tsr_infer::cancel::{CancelReason, CancelToken};
use tsr_infer::models::{ModelEntry, ModelFile, Role, VerifyError};
use tsr_net::download::{Approval, DownloadConfig, DownloadError, FileOutcome, download_file, download_model};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    Range,
    IgnoreRange,
    WrongRange,
    /// First response stops after half the body (connection dropped), later ones are fine.
    DropOnce,
    Corrupt,
    TooLong,
    RedirectOffsite,
    RedirectLocal,
}

struct Server {
    port: u16,
    requests: Arc<AtomicU32>,
    ranges: Arc<Mutex<Vec<Option<String>>>>,
    stop: Arc<AtomicBool>,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
    }
}

fn serve(body: Vec<u8>, mode: Mode) -> Server {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let requests = Arc::new(AtomicU32::new(0));
    let ranges = Arc::new(Mutex::new(Vec::new()));
    let stop = Arc::new(AtomicBool::new(false));
    let (r2, g2, s2) = (requests.clone(), ranges.clone(), stop.clone());
    std::thread::spawn(move || {
        let dropped = AtomicBool::new(false);
        for c in l.incoming() {
            if s2.load(Ordering::Relaxed) {
                break;
            }
            let Ok(mut c) = c else { continue };
            let mut rd = BufReader::new(c.try_clone().unwrap());
            let mut line = String::new();
            if rd.read_line(&mut line).is_err() {
                continue;
            }
            let path = line.split_whitespace().nth(1).unwrap_or("/").to_owned();
            let mut range = None;
            loop {
                let mut h = String::new();
                if rd.read_line(&mut h).is_err() || h.trim().is_empty() {
                    break;
                }
                if let Some((k, v)) = h.split_once(':')
                    && k.eq_ignore_ascii_case("range")
                {
                    range = Some(v.trim().to_owned());
                }
            }
            r2.fetch_add(1, Ordering::Relaxed);
            g2.lock().unwrap().push(range.clone());
            let start: usize = range
                .as_deref()
                .and_then(|r| r.strip_prefix("bytes="))
                .and_then(|r| r.trim_end_matches('-').parse().ok())
                .unwrap_or(0);
            let total = body.len();
            let send = |c: &mut TcpStream, status: &str, extra: &str, data: &[u8]| {
                let _ = write!(
                    c,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n",
                    data.len()
                );
                let _ = c.write_all(data);
            };
            match mode {
                Mode::RedirectOffsite => {
                    let _ = write!(
                        c,
                        "HTTP/1.1 302 Found\r\nLocation: https://evil.example/f\r\nContent-Length: 0\r\n\r\n"
                    );
                }
                Mode::RedirectLocal if !path.starts_with("/real") => {
                    let _ = write!(
                        c,
                        "HTTP/1.1 302 Found\r\nLocation: /real{path}\r\nContent-Length: 0\r\n\r\n"
                    );
                }
                Mode::IgnoreRange => send(&mut c, "200 OK", "", &body),
                Mode::WrongRange if start > 0 => {
                    let s = start / 2;
                    send(
                        &mut c,
                        "206 Partial Content",
                        &format!("Content-Range: bytes {s}-{}/{total}\r\n", total - 1),
                        &body[s..],
                    );
                }
                Mode::Corrupt => {
                    let mut b = body.clone();
                    b[total / 2] ^= 0xff;
                    send(&mut c, "200 OK", "", &b);
                }
                Mode::TooLong => {
                    let mut b = body.clone();
                    b.extend_from_slice(&[0u8; 64 << 10]);
                    send(&mut c, "200 OK", "", &b);
                }
                Mode::DropOnce if !dropped.swap(true, Ordering::Relaxed) => {
                    // Promise everything, deliver half, hang up.
                    let _ = write!(
                        c,
                        "HTTP/1.1 200 OK\r\nContent-Length: {total}\r\nConnection: close\r\n\r\n"
                    );
                    let _ = c.write_all(&body[..total / 2]);
                }
                _ if start > 0 => send(
                    &mut c,
                    "206 Partial Content",
                    &format!("Content-Range: bytes {start}-{}/{total}\r\n", total - 1),
                    &body[start..],
                ),
                _ => send(&mut c, "200 OK", "", &body),
            }
        }
    });
    Server {
        port,
        requests,
        ranges,
        stop,
    }
}

fn content() -> Vec<u8> {
    // 1.5 MiB of deterministic, non-repeating bytes.
    let mut x = 0x1234_5678u32;
    (0..(3 << 19))
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            x as u8
        })
        .collect()
}

fn entry(body: &[u8], path: &str) -> ModelEntry {
    ModelEntry {
        id: "test-model".into(),
        role: Role::Llm,
        repo: "t/m".into(),
        revision: "0".repeat(40),
        license: "MIT".into(),
        dims: None,
        files: vec![ModelFile {
            path: path.into(),
            size: body.len() as u64,
            sha256: Some(Sha256::digest(body).iter().map(|b| format!("{b:02x}")).collect()),
            git_blob_sha1: None,
        }],
    }
}

fn cfg(dir: &Path, s: &Server) -> DownloadConfig {
    let mut c = DownloadConfig::new(dir.to_path_buf());
    c.test_base_url = Some(format!("http://127.0.0.1:{}", s.port));
    c.stall_timeout = Duration::from_secs(5);
    c
}

fn ok() -> Approval {
    Approval::by_user(&["test-model"], 0)
}

fn leftovers(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".part") || n.ends_with(".part.json"))
        .collect()
}

#[test]
fn fresh_download_is_verified_and_renamed() {
    let body = content();
    let s = serve(body.clone(), Mode::Range);
    let d = tempfile::tempdir().unwrap();
    let e = entry(&body, "f.bin");
    let mut last = 0;
    let out = download_model(&e, &ok(), &cfg(d.path(), &s), &CancelToken::new(), &mut |p| {
        last = p.done
    })
    .unwrap();
    assert!(matches!(
        out[0],
        FileOutcome::Downloaded {
            resumed_from: 0,
            restarts: 0,
            ..
        }
    ));
    assert_eq!(std::fs::read(d.path().join("f.bin")).unwrap(), body);
    assert_eq!(last, body.len() as u64);
    assert!(leftovers(d.path()).is_empty());
}

#[test]
fn interrupted_transfer_resumes_with_a_range_request() {
    let body = content();
    let s = serve(body.clone(), Mode::DropOnce);
    let d = tempfile::tempdir().unwrap();
    let e = entry(&body, "f.bin");
    download_model(&e, &ok(), &cfg(d.path(), &s), &CancelToken::new(), &mut |_| {}).unwrap();
    assert_eq!(std::fs::read(d.path().join("f.bin")).unwrap(), body);
    let ranges = s.ranges.lock().unwrap().clone();
    assert_eq!(ranges[0], None);
    assert_eq!(
        ranges[1].as_deref(),
        Some(format!("bytes={}-", body.len() / 2).as_str())
    );
}

#[test]
fn cancelled_download_resumes_in_a_later_call() {
    let body = content();
    let s = serve(body.clone(), Mode::Range);
    let d = tempfile::tempdir().unwrap();
    let e = entry(&body, "f.bin");
    let c = CancelToken::new();
    let r = download_file(&e, &e.files[0], &ok(), &cfg(d.path(), &s), &c, &mut |p| {
        if p.done > 0 {
            c.cancel(CancelReason::Cancelled);
        }
    });
    assert_eq!(r.unwrap_err(), DownloadError::Cancelled);
    assert!(!d.path().join("f.bin").exists(), "no final file before verification");
    let part = std::fs::metadata(d.path().join("f.bin.part"))
        .map(|m| m.len())
        .unwrap_or(0);
    let out = download_file(
        &e,
        &e.files[0],
        &ok(),
        &cfg(d.path(), &s),
        &CancelToken::new(),
        &mut |_| {},
    )
    .unwrap();
    if part > 0 {
        assert!(
            matches!(out, FileOutcome::Downloaded { resumed_from, .. } if resumed_from == part),
            "{out:?}"
        );
    }
    assert_eq!(std::fs::read(d.path().join("f.bin")).unwrap(), body);
}

/// Leave a half part file plus its sidecar, as an interrupted earlier run would.
fn plant_part(dir: &Path, e: &ModelEntry, body: &[u8], cut: usize) {
    std::fs::write(dir.join("f.bin.part"), &body[..cut]).unwrap();
    let meta = serde_json::json!({"url": e.url(&e.files[0]), "size": body.len(), "digest": e.files[0].sha256});
    std::fs::write(dir.join("f.bin.part.json"), meta.to_string()).unwrap();
}

#[test]
fn server_ignoring_range_restarts_from_zero() {
    let body = content();
    let s = serve(body.clone(), Mode::IgnoreRange);
    let d = tempfile::tempdir().unwrap();
    let e = entry(&body, "f.bin");
    plant_part(d.path(), &e, &body, 1000);
    let out = download_model(&e, &ok(), &cfg(d.path(), &s), &CancelToken::new(), &mut |_| {}).unwrap();
    assert!(
        matches!(out[0], FileOutcome::Downloaded { resumed_from: 0, .. }),
        "{out:?}"
    );
    assert_eq!(
        std::fs::read(d.path().join("f.bin")).unwrap(),
        body,
        "200 body not appended to the part"
    );
}

#[test]
fn wrong_content_range_is_never_spliced_in() {
    let body = content();
    let s = serve(body.clone(), Mode::WrongRange);
    let d = tempfile::tempdir().unwrap();
    let e = entry(&body, "f.bin");
    plant_part(d.path(), &e, &body, 4000);
    download_model(&e, &ok(), &cfg(d.path(), &s), &CancelToken::new(), &mut |_| {}).unwrap();
    assert_eq!(std::fs::read(d.path().join("f.bin")).unwrap(), body);
}

#[test]
fn corrupt_content_fails_verification_and_leaves_nothing_usable() {
    let body = content();
    let s = serve(body.clone(), Mode::Corrupt);
    let d = tempfile::tempdir().unwrap();
    let e = entry(&body, "f.bin");
    let r = download_model(&e, &ok(), &cfg(d.path(), &s), &CancelToken::new(), &mut |_| {});
    assert_eq!(r.unwrap_err(), DownloadError::Verify(VerifyError::DigestMismatch));
    assert!(!d.path().join("f.bin").exists());
    assert!(leftovers(d.path()).is_empty());
    assert_eq!(
        s.requests.load(Ordering::Relaxed),
        2,
        "one full retry from zero, then stop"
    );
}

#[test]
fn oversized_body_is_rejected() {
    let body = content();
    let s = serve(body.clone(), Mode::TooLong);
    let d = tempfile::tempdir().unwrap();
    let e = entry(&body, "f.bin");
    assert_eq!(
        download_model(&e, &ok(), &cfg(d.path(), &s), &CancelToken::new(), &mut |_| {}).unwrap_err(),
        DownloadError::TooLong
    );
    assert!(!d.path().join("f.bin").exists());
}

#[test]
fn nothing_is_fetched_without_approval() {
    let body = content();
    let s = serve(body.clone(), Mode::Range);
    let d = tempfile::tempdir().unwrap();
    let e = entry(&body, "f.bin");
    let other = Approval::by_user(&["another-model"], 0);
    assert_eq!(
        download_model(&e, &other, &cfg(d.path(), &s), &CancelToken::new(), &mut |_| {}).unwrap_err(),
        DownloadError::NotApproved
    );
    assert_eq!(s.requests.load(Ordering::Relaxed), 0);
}

#[test]
fn existing_files_are_kept_or_refused_never_overwritten() {
    let body = content();
    let s = serve(body.clone(), Mode::Range);
    let d = tempfile::tempdir().unwrap();
    let e = entry(&body, "f.bin");
    std::fs::write(d.path().join("f.bin"), &body).unwrap();
    let out = download_model(&e, &ok(), &cfg(d.path(), &s), &CancelToken::new(), &mut |_| {}).unwrap();
    assert!(matches!(out[0], FileOutcome::AlreadyPresent { .. }));
    std::fs::write(d.path().join("f.bin"), b"the user's own different file").unwrap();
    assert_eq!(
        download_model(&e, &ok(), &cfg(d.path(), &s), &CancelToken::new(), &mut |_| {}).unwrap_err(),
        DownloadError::ExistsDifferent
    );
    assert_eq!(
        std::fs::read(d.path().join("f.bin")).unwrap(),
        b"the user's own different file"
    );
    assert_eq!(s.requests.load(Ordering::Relaxed), 0, "no network for either case");
}

#[test]
fn traversal_paths_are_refused_before_any_request() {
    let body = content();
    let s = serve(body.clone(), Mode::Range);
    let d = tempfile::tempdir().unwrap();
    for p in ["../escape.bin", "C:\\Windows\\x.bin", "sub/../../x.bin", "nul"] {
        let e = entry(&body, p);
        assert!(
            matches!(
                download_model(&e, &ok(), &cfg(d.path(), &s), &CancelToken::new(), &mut |_| {}),
                Err(DownloadError::UnsafePath(_))
            ),
            "{p}"
        );
    }
    assert_eq!(s.requests.load(Ordering::Relaxed), 0);
}

#[test]
fn nested_manifest_paths_land_inside_the_folder() {
    let body = content();
    let s = serve(body.clone(), Mode::Range);
    let d = tempfile::tempdir().unwrap();
    let e = entry(&body, "onnx/model.onnx");
    download_model(&e, &ok(), &cfg(d.path(), &s), &CancelToken::new(), &mut |_| {}).unwrap();
    assert_eq!(std::fs::read(d.path().join("onnx").join("model.onnx")).unwrap(), body);
}

#[test]
fn redirects_are_followed_only_to_allowed_hosts() {
    let body = content();
    let d = tempfile::tempdir().unwrap();
    let e = entry(&body, "f.bin");
    let s = serve(body.clone(), Mode::RedirectOffsite);
    assert_eq!(
        download_model(&e, &ok(), &cfg(d.path(), &s), &CancelToken::new(), &mut |_| {}).unwrap_err(),
        DownloadError::HostNotAllowed("evil.example".into())
    );
    let s = serve(body.clone(), Mode::RedirectLocal);
    download_model(&e, &ok(), &cfg(d.path(), &s), &CancelToken::new(), &mut |_| {}).unwrap();
    assert_eq!(std::fs::read(d.path().join("f.bin")).unwrap(), body);
}

#[test]
fn stale_part_of_another_file_is_discarded() {
    let body = content();
    let s = serve(body.clone(), Mode::Range);
    let d = tempfile::tempdir().unwrap();
    let e = entry(&body, "f.bin");
    std::fs::write(d.path().join("f.bin.part"), b"garbage from some other download").unwrap();
    std::fs::write(
        d.path().join("f.bin.part.json"),
        b"{\"url\":\"x\",\"size\":1,\"digest\":\"y\"}",
    )
    .unwrap();
    let out = download_model(&e, &ok(), &cfg(d.path(), &s), &CancelToken::new(), &mut |_| {}).unwrap();
    assert!(matches!(out[0], FileOutcome::Downloaded { resumed_from: 0, .. }));
    assert_eq!(std::fs::read(d.path().join("f.bin")).unwrap(), body);
    assert_eq!(s.ranges.lock().unwrap()[0], None, "no range request for a foreign part");
}
