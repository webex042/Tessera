//! REAL end-to-end capture on this desktop: DPAPI-keyed store, OS-events thread, gates, UIA,
//! redaction, sessions, store writer. Capture is restricted to notepad.exe (diagnostics filter) so
//! nothing else on screen is read; the data dir is a temp folder. Opens a Notepad window:
//!     cargo test -p tsr-capture --test capture_real -- --ignored --nocapture
#![cfg(windows)]

use std::time::{Duration, Instant};

use tsr_capture::engine::CaptureCore;
use tsr_capture::platform_win::{WinPlatform, spawn_capture};
use tsr_capture::sink::StoreSink;
use tsr_guard::blocklist::{Blocklist, builtin_rules};
use tsr_guard::redact::Options;
use tsr_store::items::search;
use tsr_store::keystore::open_store;

const CANARY_WORD: &str = "tsrcapturecanary";
const FAKE_OTP: &str = "739104";
const FAKE_CARD: &str = "4111 1111 1111 1111";

#[test]
#[ignore = "opens a Notepad window and reads it"]
fn notepad_note_is_captured_redacted_and_stored() {
    let running = std::process::Command::new("tasklist")
        .args(["/fi", "imagename eq notepad.exe", "/nh"])
        .output()
        .unwrap();
    if String::from_utf8_lossy(&running.stdout)
        .to_lowercase()
        .contains("notepad.exe")
    {
        eprintln!("SKIPPED: Notepad is already open");
        return;
    }
    let data = tempfile::tempdir().unwrap();
    let conn = open_store(data.path()).expect("store");

    let handle = spawn_capture(move || {
        let (bl, _) = Blocklist::compile(&builtin_rules());
        let mut core = CaptureCore::new(WinPlatform::new()?, StoreSink::spawn(conn)?, bl, Options::default());
        core.only_exes = Some(vec!["notepad.exe".into()]);
        Ok(core)
    })
    .expect("capture thread");

    let note = data.path().join("tsr-capture-note.txt");
    std::fs::write(
        &note,
        format!("Meeting notes {CANARY_WORD}\nSend the revised budget to Ravi by Friday\nYour OTP is {FAKE_OTP}\nCard {FAKE_CARD}\n"),
    )
    .unwrap();
    let mut child = std::process::Command::new("notepad.exe").arg(&note).spawn().unwrap();
    handle.resume();

    // Wait for at least one snapshot of the Notepad window.
    let started = Instant::now();
    std::thread::sleep(Duration::from_secs(12));
    let core = handle.shutdown();
    let took = started.elapsed();
    let stats = core.stats;
    let CaptureCore { sink, .. } = core;
    let conn = sink.finish();
    let _ = std::process::Command::new("taskkill")
        .args(["/im", "notepad.exe", "/fi", "WINDOWTITLE eq tsr-capture-note*", "/f"])
        .output();
    let _ = child.kill();
    let _ = child.wait();

    println!("REAL capture stats after {took:?}: {stats:?}");
    let hits = search(&conn, CANARY_WORD, 5).unwrap();
    println!("REAL search hits for canary word: {}", hits.len());
    assert_eq!(hits.len(), 1, "note not stored");
    let (text, acq, exe): (String, String, String) = conn
        .query_row(
            "SELECT items.text, items.acquisition, sources.app_exe FROM items JOIN sources ON sources.id = items.source_id",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    println!(
        "REAL stored: acquisition={acq} exe={exe} chars={}",
        text.chars().count()
    );
    assert!(exe.eq_ignore_ascii_case("notepad.exe"), "only Notepad may be captured");
    assert!(
        text.contains("[REDACTED:otp]") && text.contains("[REDACTED:card]"),
        "not redacted"
    );
    assert!(
        !text.contains(FAKE_OTP) && !text.contains("4111"),
        "secret stored in clear"
    );
    let others: i64 = conn
        .query_row(
            "SELECT count(*) FROM sources WHERE lower(app_exe) <> 'notepad.exe'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(others, 0);
    drop(conn);

    // On disk: encrypted; neither the canary word nor the fake secrets in clear.
    for e in std::fs::read_dir(data.path()).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_some_and(|x| x == "txt") {
            continue; // the fixture note itself
        }
        let bytes = std::fs::read(&p).unwrap();
        for needle in [CANARY_WORD.as_bytes(), FAKE_OTP.as_bytes()] {
            assert!(
                !bytes.windows(needle.len()).any(|w| w == needle),
                "{} contains plaintext",
                p.display()
            );
        }
    }
}
