//! REAL desktop test of global hotkeys on the T1 events thread: registers Ctrl+Alt+Shift+F9
//! (unlikely to be taken), presses it with synthesized key events, and expects the Hotkey event.
//! Ignored by default because it sends key events to the desktop:
//!     cargo test -p tsr-win --test hotkey_real -- --ignored --nocapture
#![cfg(windows)]

use std::sync::mpsc;
use std::time::{Duration, Instant};

use tsr_win::events::{Hotkey, MOD_ALT, MOD_CONTROL, MOD_SHIFT, OsEvent, spawn_with};
use windows::Win32::UI::Input::KeyboardAndMouse::{KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, keybd_event};

const VK_F9: u32 = 0x78;

fn press(keys: &[u8]) {
    // SAFETY: synthesized key events; every key pressed is released in reverse order.
    unsafe {
        for k in keys {
            keybd_event(*k, 0, KEYBD_EVENT_FLAGS(0), 0);
        }
        for k in keys.iter().rev() {
            keybd_event(*k, 0, KEYEVENTF_KEYUP, 0);
        }
    }
}

#[test]
#[ignore = "sends key events to the desktop"]
fn registered_hotkey_fires() {
    let (tx, rx) = mpsc::channel();
    let hk = Hotkey {
        id: 7,
        modifiers: MOD_CONTROL | MOD_ALT | MOD_SHIFT,
        vk: VK_F9,
        alternates: &[],
    };
    let t = spawn_with(tx, vec![hk]).expect("events thread");
    let end = Instant::now() + Duration::from_secs(3);
    let mut registered = None;
    while Instant::now() < end && registered.is_none() {
        if let Ok(e) = rx.recv_timeout(Duration::from_millis(100))
            && matches!(e, OsEvent::HotkeyRegistered { .. } | OsEvent::HotkeyUnavailable { .. })
        {
            registered = Some(e);
        }
    }
    println!("REAL registration: {registered:?}");
    assert!(matches!(registered, Some(OsEvent::HotkeyRegistered { id: 7, .. })));

    press(&[0x11, 0x12, 0x10, VK_F9 as u8]);
    let end = Instant::now() + Duration::from_secs(3);
    let mut fired = false;
    while Instant::now() < end && !fired {
        if let Ok(e) = rx.recv_timeout(Duration::from_millis(100)) {
            fired = e == OsEvent::Hotkey { id: 7 };
        }
    }
    t.stop();
    assert!(fired, "WM_HOTKEY did not arrive");
}
