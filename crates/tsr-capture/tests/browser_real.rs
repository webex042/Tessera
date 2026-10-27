//! REAL browsers: private-state and URL facts from live Chrome and Edge windows (throwaway
//! profiles, local synthetic page). Opens and closes browser windows, so ignored by default:
//!     cargo test -p tsr-capture --test browser_real -- --ignored --nocapture --test-threads=1
#![cfg(windows)]

use std::path::Path;
use std::process::Child;
use std::time::{Duration, Instant};

use tsr_capture::uia::UiaReader;
use tsr_guard::blocklist::PrivateState;
use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowTextLengthW, GetWindowThreadProcessId, IsWindowVisible,
};
use windows::core::BOOL;

/// The fixture as a normal `file:///C:/...` URL. `canonicalize` yields a verbatim (`\\?\`) path;
/// Chrome opens that but shows nothing in the address bar, which at first looked like a UIA
/// problem (2026-10-27).
fn fixture() -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/screens/chat.html")
        .canonicalize()
        .unwrap();
    let s = p.to_string_lossy().replace('\\', "/");
    format!("file:///{}", s.trim_start_matches("//?/"))
}

fn window_of(pid: u32) -> Option<HWND> {
    unsafe extern "system" fn cb(h: HWND, l: LPARAM) -> BOOL {
        // SAFETY: l is &mut (u32, Option<HWND>) for the duration of EnumWindows.
        let st = unsafe { &mut *(l.0 as *mut (u32, Option<HWND>)) };
        let mut p = 0u32;
        unsafe { GetWindowThreadProcessId(h, Some(&mut p)) };
        if p == st.0 && unsafe { IsWindowVisible(h).as_bool() && GetWindowTextLengthW(h) > 0 } {
            st.1 = Some(h);
            return BOOL(0);
        }
        BOOL(1)
    }
    let mut st: (u32, Option<HWND>) = (pid, None);
    unsafe {
        let _ = EnumWindows(Some(cb), LPARAM(&mut st as *mut _ as isize));
    }
    st.1
}

struct Browser {
    child: Child,
    /// Kept so the throwaway profile outlives the browser.
    _profile: tempfile::TempDir,
}
impl Drop for Browser {
    fn drop(&mut self) {
        let _ = std::process::Command::new("taskkill")
            .args(["/pid", &self.child.id().to_string(), "/T", "/F"])
            .output();
    }
}

fn launch(exe: &str, private_flag: Option<&str>) -> Browser {
    let prof = tempfile::tempdir().unwrap();
    let mut c = std::process::Command::new(exe);
    c.arg(format!("--user-data-dir={}", prof.path().display())).args([
        "--no-first-run",
        "--no-default-browser-check",
        "--new-window",
    ]);
    if let Some(f) = private_flag {
        c.arg(f);
    }
    c.arg(fixture());
    Browser {
        child: c.spawn().unwrap(),
        _profile: prof,
    }
}

fn check(name: &str, exe: &str, private_flag: Option<&str>, want: PrivateState) {
    if !Path::new(exe).exists() {
        println!("SKIPPED {name}: not installed");
        return;
    }
    let b = launch(exe, private_flag);
    let end = Instant::now() + Duration::from_secs(15);
    let hwnd = loop {
        if let Some(h) = window_of(b.child.id()) {
            break h;
        }
        assert!(Instant::now() < end, "{name}: no window");
        std::thread::sleep(Duration::from_millis(200));
    };
    std::thread::sleep(Duration::from_secs(3)); // let the page and toolbar settle
    let r = UiaReader::new().unwrap();
    let mut facts = r.browser(hwnd);
    for _ in 0..5 {
        if facts.as_ref().is_ok_and(|f| f.url.is_some()) {
            break;
        }
        r.wait_idle(Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(500));
        facts = r.browser(hwnd);
    }
    let f = facts.unwrap_or_else(|e| panic!("{name}: {e}"));
    let url = f.url.as_ref().map(|u| u.expose().clone()).unwrap_or_default();
    println!(
        "REAL {name}: private={:?} url_has_fixture={}",
        f.private,
        url.contains("chat.html")
    );
    assert_eq!(f.private, want, "{name}");
    assert!(url.contains("chat.html"), "{name}: address bar not read");
}

#[test]
#[ignore = "opens browser windows"]
fn chrome_and_edge_private_state() {
    let chrome = r"C:\Program Files\Google\Chrome\Application\chrome.exe";
    let edge = r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe";
    check("chrome normal", chrome, None, PrivateState::NotPrivate);
    check("chrome incognito", chrome, Some("--incognito"), PrivateState::Private);
    check("edge normal", edge, None, PrivateState::NotPrivate);
    check("edge inprivate", edge, Some("--inprivate"), PrivateState::Private);
}
