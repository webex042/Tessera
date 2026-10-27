//! REAL desktop test of the T1 events thread. Opens and closes a Notepad window, so it is ignored
//! by default:  cargo test -p tsr-win --test events_real -- --ignored --nocapture
#![cfg(windows)]

use std::sync::mpsc;
use std::time::{Duration, Instant};

use tsr_win::events::{OsEvent, spawn};
use tsr_win::query::{process_image, window_facts};

fn wait_for(rx: &mpsc::Receiver<OsEvent>, secs: u64, mut pred: impl FnMut(&OsEvent) -> bool) -> Option<OsEvent> {
    let end = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < end {
        if let Ok(e) = rx.recv_timeout(Duration::from_millis(100))
            && pred(&e)
        {
            return Some(e);
        }
    }
    None
}

#[test]
#[ignore = "opens a Notepad window on the desktop"]
fn foreground_title_and_power_events_arrive() {
    let running = std::process::Command::new("tasklist")
        .args(["/fi", "imagename eq notepad.exe", "/nh"])
        .output()
        .unwrap();
    if String::from_utf8_lossy(&running.stdout)
        .to_lowercase()
        .contains("notepad.exe")
    {
        eprintln!("SKIPPED: Notepad is already open (a new file would join the user's window)");
        return;
    }
    let (tx, rx) = mpsc::channel();
    let t = spawn(tx).expect("events thread");

    // RegisterPowerSettingNotification reports the current AC/DC state immediately.
    let p = wait_for(&rx, 3, |e| matches!(e, OsEvent::PowerSource { .. }));
    println!("REAL initial power event: {p:?}");
    assert!(p.is_some(), "no initial power-source notification");

    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("tsr-events-fixture.txt");
    std::fs::write(&file, "synthetic fixture").unwrap();
    let mut launcher = std::process::Command::new("notepad.exe").arg(&file).spawn().unwrap();

    let fg = wait_for(&rx, 10, |e| match e {
        OsEvent::Foreground { pid, .. } => {
            process_image(*pid).is_some_and(|p| p.to_lowercase().ends_with("notepad.exe"))
        }
        _ => false,
    });
    println!("REAL foreground event: {fg:?}");
    let Some(OsEvent::Foreground { hwnd, pid }) = fg else {
        panic!("no foreground event for Notepad")
    };
    // Close the window even if an assertion below fails.
    struct Kill(u32);
    impl Drop for Kill {
        fn drop(&mut self) {
            let _ = std::process::Command::new("taskkill")
                .args(["/pid", &self.0.to_string(), "/f"])
                .output();
        }
    }
    let _kill = Kill(pid);

    // Rename the window from outside (WM_SETTEXT) and expect a TitleChanged for it.
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::SetWindowTextW;
    use windows::core::w;
    unsafe { SetWindowTextW(HWND(hwnd as *mut _), w!("tsr renamed window")).unwrap() };
    let title = wait_for(&rx, 5, |e| *e == OsEvent::TitleChanged { hwnd });
    println!("REAL title event: {title:?}");
    assert!(title.is_some(), "no TitleChanged event");
    // Windows 11 Notepad rewrites its own title from the open tab right away, so only check that
    // the facts query works on the window, not that our title stuck.
    let facts = window_facts(hwnd).unwrap();
    assert!(facts.exe.eq_ignore_ascii_case("notepad.exe"), "{facts:?}");
    t.stop();
    // Close the window first: Windows 11's notepad.exe is usually a launcher that exits once the
    // app window exists, but sometimes it is the app itself, and waiting on it would hang.
    drop(_kill);
    let _ = launcher.kill();
    let _ = launcher.wait();
}
