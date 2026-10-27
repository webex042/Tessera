//! Capture engine end to end with a SIMULATED platform (scripted windows and reads) and the REAL
//! store writer + encrypted database. Plan §4.2, §5, §6, §8.1; Stage 1 exit criteria.

use std::collections::HashMap;

use rusqlite::Connection;
use tsr_capture::engine::{BrowserInfo, CaptureCore, Control, Foreground, OsState, Platform, ReadOutcome, Signal};
use tsr_capture::gate::UserActivity;
use tsr_capture::sink::StoreSink;
use tsr_core::Sensitive;
use tsr_guard::blocklist::{Blocklist, PrivateState, builtin_rules};
use tsr_guard::redact::Options;
use tsr_store::blocklist::blocked_counts;
use tsr_store::items::search;
use tsr_store::{KEY_LEN, migrate::migrate, open_encrypted};

#[derive(Clone)]
enum Read {
    Text(&'static str),
    TimedOut,
    Failed,
}

#[derive(Default)]
struct MockPlatform {
    windows: Vec<(Foreground, Read, Read)>, // window, UIA result, OCR result
    current: usize,
    browsers: HashMap<isize, BrowserInfo>,
    os: Option<OsState>,
    uia_reads: u32,
    ocr_reads: u32,
}

impl MockPlatform {
    fn window(mut self, hwnd: isize, exe: &str, title: &str, uia: Read, ocr: Read) -> Self {
        self.windows.push((
            Foreground {
                hwnd,
                pid: 100 + hwnd as u32,
                exe: exe.into(),
                exe_path: format!(r"C:\Apps\{exe}"),
                title: Sensitive::new(title.into()),
                minimized: false,
            },
            uia,
            ocr,
        ));
        self
    }

    fn browser(mut self, hwnd: isize, url: Option<&str>, private: PrivateState) -> Self {
        self.browsers.insert(
            hwnd,
            BrowserInfo {
                url: url.map(|u| Sensitive::new(u.into())),
                private,
            },
        );
        self
    }
}

fn out(r: &Read) -> ReadOutcome {
    match r {
        Read::Text(t) => ReadOutcome::Text(Sensitive::new((*t).into())),
        Read::TimedOut => ReadOutcome::TimedOut,
        Read::Failed => ReadOutcome::Failed,
    }
}

impl Platform for MockPlatform {
    fn foreground(&mut self) -> Option<Foreground> {
        self.windows.get(self.current).map(|w| w.0.clone())
    }
    fn os_state(&mut self) -> OsState {
        self.os.unwrap_or(OsState {
            idle_ms: 1_000,
            activity: UserActivity::Normal,
        })
    }
    fn browser(&mut self, w: &Foreground) -> Option<BrowserInfo> {
        self.browsers.get(&w.hwnd).cloned()
    }
    fn read_uia(&mut self, w: &Foreground) -> ReadOutcome {
        self.uia_reads += 1;
        out(&self.windows.iter().find(|x| x.0.hwnd == w.hwnd).unwrap().1)
    }
    fn read_ocr(&mut self, w: &Foreground) -> ReadOutcome {
        self.ocr_reads += 1;
        out(&self.windows.iter().find(|x| x.0.hwnd == w.hwnd).unwrap().2)
    }
}

fn db() -> (tempfile::TempDir, Connection) {
    let d = tempfile::tempdir().unwrap();
    let mut c = open_encrypted(&d.path().join("t.db"), &[9u8; KEY_LEN]).unwrap();
    migrate(&mut c).unwrap();
    (d, c)
}

fn engine(p: MockPlatform, conn: Connection) -> CaptureCore<MockPlatform, StoreSink> {
    let (bl, _) = Blocklist::compile(&builtin_rules());
    CaptureCore::new(p, StoreSink::spawn(conn).unwrap(), bl, Options::default())
}

/// Run ticks every second from `from` to `to` (ms).
fn run(e: &mut CaptureCore<MockPlatform, StoreSink>, from: i64, to: i64) {
    let mut t = from;
    while t <= to {
        e.tick(t);
        t += 1_000;
    }
}

fn finish(mut e: CaptureCore<MockPlatform, StoreSink>, at: i64) -> (Connection, MockPlatform) {
    e.handle(Control::Shutdown, at);
    let CaptureCore { platform, sink, .. } = e;
    (sink.finish(), platform)
}

fn items(c: &Connection) -> Vec<(String, String, i64, String)> {
    let mut st = c
        .prepare("SELECT items.text, items.acquisition, items.redaction_count, sources.window_title FROM items JOIN sources ON sources.id = items.source_id ORDER BY items.id")
        .unwrap();
    st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

const NOTES: &str =
    "Meeting notes\nSend the revised budget to Ravi by Friday\nYour OTP is 482913\nCard 4111 1111 1111 1111";

#[test]
fn starts_paused_and_stores_nothing_until_resumed() {
    let (_d, c) = db();
    let p = MockPlatform::default().window(1, "notepad.exe", "notes.txt - Notepad", Read::Text(NOTES), Read::Failed);
    let mut e = engine(p, c);
    assert!(e.is_paused());
    run(&mut e, 0, 20_000);
    let (c, p) = finish(e, 20_000);
    assert_eq!(p.uia_reads, 0, "nothing is read while paused");
    assert!(items(&c).is_empty());
}

#[test]
fn text_is_redacted_before_storage_and_searchable() {
    let (_d, c) = db();
    // A card number in the title (an "OTP" title would be blocked outright by the blocklist).
    let p = MockPlatform::default().window(
        1,
        "notepad.exe",
        "card 5500 0000 0000 0004.txt - Notepad",
        Read::Text(NOTES),
        Read::Failed,
    );
    let mut e = engine(p, c);
    e.handle(Control::Resume, 0);
    run(&mut e, 0, 12_000);
    let (c, _) = finish(e, 12_000);
    let rows = items(&c);
    assert_eq!(rows.len(), 1);
    let (text, acq, redactions, title) = &rows[0];
    assert!(
        text.contains("[REDACTED:otp]") && text.contains("[REDACTED:card]"),
        "{text}"
    );
    assert!(
        !text.contains("482913") && !text.contains("4111"),
        "secret stored: {text}"
    );
    assert!(
        title.contains("[REDACTED:card]") && !title.contains("5500"),
        "title must be redacted too: {title}"
    );
    assert_eq!(acq, "uia");
    assert!(*redactions >= 3);
    assert_eq!(search(&c, "budget ravi", 5).unwrap().len(), 1);
}

#[test]
fn blocklisted_app_stores_nothing_and_audits_once() {
    let (_d, c) = db();
    let p = MockPlatform::default().window(
        1,
        "KeePassXC.exe",
        "Passwords.kdbx - KeePassXC",
        Read::Text("bank: hunter2"),
        Read::Failed,
    );
    let mut e = engine(p, c);
    e.handle(Control::Resume, 0);
    run(&mut e, 0, 60_000);
    let (c, p) = finish(e, 60_000);
    assert_eq!(p.uia_reads, 0, "blocked windows are never read");
    assert!(items(&c).is_empty());
    assert_eq!(
        blocked_counts(&c, 0).unwrap(),
        vec![("builtin.process.keepass".to_string(), 1)]
    );
}

#[test]
fn ordinary_windows_with_login_in_the_title_are_captured() {
    let (_d, c) = db();
    let p = MockPlatform::default().window(
        1,
        "WINWORD.EXE",
        "Login flow redesign.docx - Word",
        Read::Text("Login flow redesign: three screens"),
        Read::Failed,
    );
    let mut e = engine(p, c);
    e.handle(Control::Resume, 0);
    run(&mut e, 0, 6_000);
    let (c, _) = finish(e, 6_000);
    assert_eq!(items(&c).len(), 1);
}

#[test]
fn browsers_need_known_private_state_and_urls_are_cleaned() {
    let (_d, c) = db();
    let p = MockPlatform::default()
        .window(
            1,
            "chrome.exe",
            "Docs - Google Chrome",
            Read::Text("Quarterly plan draft"),
            Read::Failed,
        )
        .browser(
            1,
            Some("https://docs.example/d/1?token=SECRETTOKEN123&tab=2"),
            PrivateState::NotPrivate,
        );
    let mut e = engine(p, c);
    e.handle(Control::Resume, 0);
    run(&mut e, 0, 6_000);
    let (c, _) = finish(e, 6_000);
    let url: String = c.query_row("SELECT url FROM sources", [], |r| r.get(0)).unwrap();
    assert_eq!(url, "https://docs.example/d/1?tab=2");

    // Unknown private state (facts unreadable) blocks the window.
    let (_d, c) = db();
    let p = MockPlatform::default().window(
        2,
        "msedge.exe",
        "News - Microsoft Edge",
        Read::Text("headline"),
        Read::Failed,
    );
    let mut e = engine(p, c);
    e.handle(Control::Resume, 0);
    run(&mut e, 0, 6_000);
    let (c, p) = finish(e, 6_000);
    assert!(items(&c).is_empty());
    assert_eq!(p.uia_reads, 0);
    assert_eq!(
        blocked_counts(&c, 0).unwrap()[0].0,
        "builtin.private.undetermined_or_private"
    );
}

#[test]
fn pause_drops_buffered_text_and_lock_flushes_it() {
    let (_d, c) = db();
    let p = MockPlatform::default().window(
        1,
        "notepad.exe",
        "a.txt - Notepad",
        Read::Text("draft that must not be kept"),
        Read::Failed,
    );
    let mut e = engine(p, c);
    e.handle(Control::Resume, 0);
    run(&mut e, 0, 3_000);
    e.handle(Control::Pause, 3_500);
    run(&mut e, 4_000, 100_000);
    let (c, _) = finish(e, 100_000);
    assert!(items(&c).is_empty(), "paused text was stored");

    let (_d, c) = db();
    let p = MockPlatform::default().window(
        1,
        "notepad.exe",
        "a.txt - Notepad",
        Read::Text("kept at lock"),
        Read::Failed,
    );
    let mut e = engine(p, c);
    e.handle(Control::Resume, 0);
    run(&mut e, 0, 3_000);
    e.handle(Control::Os(Signal::Locked), 3_500);
    run(&mut e, 4_000, 10_000);
    let CaptureCore { sink, platform, .. } = e;
    let c = sink.finish();
    assert_eq!(items(&c).len(), 1, "lock flushes immediately");
    assert_eq!(platform.uia_reads, 1, "no reads while locked");
}

#[test]
fn idle_and_fullscreen_mean_no_reads() {
    for activity in [UserActivity::Fullscreen, UserActivity::Presentation] {
        let (_d, c) = db();
        let mut p = MockPlatform::default().window(1, "notepad.exe", "a - Notepad", Read::Text("x y z"), Read::Failed);
        p.os = Some(OsState {
            idle_ms: 1_000,
            activity,
        });
        let mut e = engine(p, c);
        e.handle(Control::Resume, 0);
        run(&mut e, 0, 30_000);
        let (_, p) = finish(e, 30_000);
        assert_eq!(p.uia_reads, 0, "{activity:?}");
    }
    let (_d, c) = db();
    let mut p = MockPlatform::default().window(1, "notepad.exe", "a - Notepad", Read::Text("x y z"), Read::Failed);
    p.os = Some(OsState {
        idle_ms: 600_000,
        activity: UserActivity::Normal,
    });
    let mut e = engine(p, c);
    e.handle(Control::Resume, 0);
    run(&mut e, 0, 30_000);
    let (_, p) = finish(e, 30_000);
    assert_eq!(p.uia_reads, 0, "idle");
}

#[test]
fn uia_timeouts_fall_back_to_ocr_and_password_screens_are_skipped() {
    let (_d, c) = db();
    let p = MockPlatform::default().window(
        1,
        "WINWORD.EXE",
        "Report.docx - Word",
        Read::TimedOut,
        Read::Text("Quarterly report\nRevenue grew 12%"),
    );
    let mut e = engine(p, c);
    e.handle(Control::Resume, 0);
    run(&mut e, 0, 40_000);
    let (c, p) = finish(e, 40_000);
    let rows = items(&c);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].1, "ocr");
    assert!(
        p.ocr_reads >= 2 && p.ocr_reads <= 4,
        "OCR is rate-limited: {}",
        p.ocr_reads
    );
    assert!(
        p.uia_reads <= 3,
        "after two timeouts UIA is skipped for the window: {}",
        p.uia_reads
    );

    let (_d, c) = db();
    let p = MockPlatform::default().window(
        2,
        "someapp.exe",
        "Account - Some App",
        Read::Failed,
        Read::Text("Sign in\nPassword\n••••••••\nNext"),
    );
    let mut e = engine(p, c);
    e.handle(Control::Resume, 0);
    run(&mut e, 0, 30_000);
    let (c, _) = finish(e, 30_000);
    assert!(items(&c).is_empty(), "OCR'd password screen stored");
}
