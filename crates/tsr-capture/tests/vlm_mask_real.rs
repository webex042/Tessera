//! REAL rung-3 masking check on this desktop: a Notepad window with a synthetic note is captured
//! (WGC) and read (Windows OCR), the mask plan is applied, and the **masked pixels are OCR'd
//! again**: no secret may be readable afterwards, while ordinary text still is. Nothing is
//! written to disk except the synthetic note. No model is involved.
//!
//! Finding (2026-10-27, dev machine): Windows OCR did not read the line
//! "Card 4111 1111 1111 1111" at all, so it was not masked — masking can only hide what OCR
//! detects. The two secrets OCR did read (OTP, AWS key) were masked and unreadable afterwards.
//! The VLM answer's own redaction pass still removes a transcribed card number, but the model
//! saw it. Mitigation still to build: union the mask with rung-1 UIA element boxes.
//!     cargo test -p tsr-capture --test vlm_mask_real -- --ignored --nocapture
#![cfg(windows)]

use std::time::{Duration, Instant};

use tsr_capture::ocr::{capture_frame, ocr_lines};
use tsr_capture::vlm::{Line, apply_masks, mask_plan, verify_masked};
use tsr_guard::redact::{Options, find};
use tsr_win::query::foreground;

const CANARY: &str = "tsrmaskcanary";

fn text_of(lines: &[Line]) -> String {
    lines
        .iter()
        .map(|l| {
            l.words
                .iter()
                .map(|w| w.text.expose().as_str())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
#[ignore = "opens a Notepad window and captures it"]
fn masked_frame_hides_secrets_from_ocr() {
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
    let dir = tempfile::tempdir().unwrap();
    let note = dir.path().join("tsr-mask-note.txt");
    std::fs::write(
        &note,
        format!(
            "Meeting notes {CANARY}\r\nCard 4111 1111 1111 1111\r\nYour OTP is 739104\r\nAWS key AKIAQWERTYUIOPASDFGH\r\nLunch at noon on Friday\r\n"
        ),
    )
    .unwrap();
    // Null stdio: the launched app must not hold this test's output pipe open.
    let launcher = std::process::Command::new("notepad.exe")
        .arg(&note)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    // Closes the launcher and the window even if an assertion fails part-way.
    struct Kill(std::process::Child, Option<u32>);
    impl Drop for Kill {
        fn drop(&mut self) {
            if let Some(p) = self.1 {
                let _ = std::process::Command::new("taskkill")
                    .args(["/pid", &p.to_string(), "/f"])
                    .output();
            }
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let mut kill = Kill(launcher, None);
    eprintln!("waiting for the Notepad window");
    let end = Instant::now() + Duration::from_secs(10);
    let hwnd = loop {
        if let Some(f) = foreground()
            && f.exe.eq_ignore_ascii_case("notepad.exe")
            && f.title.contains("tsr-mask-note")
        {
            kill.1 = Some(f.pid);
            break f.hwnd;
        }
        assert!(Instant::now() < end, "Notepad window did not come to the front");
        std::thread::sleep(Duration::from_millis(200));
    };
    std::thread::sleep(Duration::from_millis(800)); // let it paint
    let h = windows::Win32::Foundation::HWND(hwnd as *mut _);
    eprintln!("capturing");
    let mut frame = capture_frame(h).expect("capture");
    eprintln!("ocr");
    let before = ocr_lines(&frame).expect("ocr");
    let before_text = text_of(&before);
    let secrets_before = find(&before_text, Options::default()).len();
    println!(
        "REAL frame {}x{}; OCR lines {}; secrets detected before masking: {secrets_before}; canary read: {}",
        frame.width,
        frame.height,
        before.len(),
        before_text.contains(CANARY)
    );
    // All synthetic: printing what OCR read helps diagnose OCR misses.
    println!(
        "REAL OCR before masking (synthetic note):
{before_text}"
    );
    assert!(
        secrets_before >= 2,
        "OCR should read the synthetic secrets first (got {secrets_before})"
    );

    let plan = mask_plan(&before, Options::default(), &[]);
    apply_masks(&mut frame, &plan.rects);
    assert!(verify_masked(&frame, &plan.rects));
    let after = ocr_lines(&frame).expect("ocr after masking");
    let after_text = text_of(&after);
    let secrets_after = find(&after_text, Options::default());
    let digits_left = after_text.contains("4111") || after_text.contains("739104") || after_text.contains("AKIA");
    println!(
        "REAL masked {} words in {} rectangles; secrets readable after masking: {}; card/otp/key digits left: {digits_left}; canary still read: {}",
        plan.masked_words,
        plan.rects.len(),
        secrets_after.len(),
        after_text.contains(CANARY)
    );
    println!(
        "REAL OCR after masking:
{after_text}"
    );
    drop(kill);
    assert!(
        secrets_after.is_empty(),
        "a secret is still readable in the masked frame"
    );
    assert!(!digits_left);
    assert!(
        after_text.contains(CANARY) || after_text.to_lowercase().contains("lunch"),
        "masking removed ordinary text too"
    );
}
