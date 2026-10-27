//! REAL draft insertion on this desktop (plan §12.3, Stage 5 exit criteria), in a window this
//! test creates (two Win32 EDIT controls: a normal one and a password one). Nothing outside the
//! test window is typed into. The clipboard's text is saved and put back; if the clipboard holds
//! anything else (an image, files) the test skips rather than lose it.
//!     cargo test -p tsr-capture --test insert_real -- --ignored --nocapture
#![cfg(windows)]

use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use tsr_capture::uia::UiaReader;
use tsr_win::clipboard::{ClipError, inspect, restore, send_paste, set_private_text, snapshot};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

static NORMAL: AtomicIsize = AtomicIsize::new(0);
static PASSWORD: AtomicIsize = AtomicIsize::new(0);
const FOCUS_NORMAL: u32 = WM_APP + 1;
const FOCUS_PASSWORD: u32 = WM_APP + 2;
const ES_PASSWORD: u32 = 0x0020;
const ES_AUTOHSCROLL: u32 = 0x0080;

unsafe extern "system" fn proc_(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    // SAFETY: focus changes on this window's own thread; everything else to the default proc.
    unsafe {
        match m {
            FOCUS_NORMAL => {
                let _ = SetFocus(Some(HWND(NORMAL.load(Ordering::Acquire) as *mut _)));
                LRESULT(0)
            }
            // Receiving our own hotkey gives this thread the right to take the foreground.
            WM_HOTKEY => {
                let _ = SetForegroundWindow(h);
                LRESULT(0)
            }
            FOCUS_PASSWORD => {
                let _ = SetFocus(Some(HWND(PASSWORD.load(Ordering::Acquire) as *mut _)));
                LRESULT(0)
            }
            _ => DefWindowProcW(h, m, w, l),
        }
    }
}

fn edit_text(h: isize) -> String {
    let mut buf = [0u16; 512];
    // SAFETY: WM_GETTEXT into a local buffer of the stated size.
    let n = unsafe {
        SendMessageW(
            HWND(h as *mut _),
            WM_GETTEXT,
            Some(WPARAM(buf.len())),
            Some(LPARAM(buf.as_mut_ptr() as isize)),
        )
    };
    String::from_utf16_lossy(&buf[..n.0 as usize])
}

/// The test window on its own thread; returns (top-level hwnd, thread id).
fn spawn_window() -> (isize, u32) {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        // SAFETY: plain window creation and a message loop on this thread.
        unsafe {
            let hinst = GetModuleHandleW(None).unwrap();
            let class = w!("TsrInsertTest");
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(proc_),
                hInstance: hinst.into(),
                lpszClassName: class,
                ..Default::default()
            });
            let top = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                class,
                w!("tsr-insert-test"),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                100,
                100,
                420,
                160,
                None,
                None,
                Some(hinst.into()),
                None,
            )
            .unwrap();
            let edit = |y: i32, style: u32| {
                CreateWindowExW(
                    WS_EX_CLIENTEDGE,
                    PCWSTR(w!("EDIT").0),
                    w!(""),
                    WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(style),
                    10,
                    y,
                    380,
                    24,
                    Some(top),
                    None,
                    Some(hinst.into()),
                    None,
                )
                .unwrap()
            };
            NORMAL.store(edit(10, ES_AUTOHSCROLL).0 as isize, Ordering::Release);
            PASSWORD.store(edit(50, ES_AUTOHSCROLL | ES_PASSWORD).0 as isize, Ordering::Release);
            // Ctrl+Alt+Shift+F10 (0x79), consumed by this window while registered.
            let _ = windows::Win32::UI::Input::KeyboardAndMouse::RegisterHotKey(
                Some(top),
                1,
                windows::Win32::UI::Input::KeyboardAndMouse::HOT_KEY_MODIFIERS(0x2 | 0x1 | 0x4 | 0x4000),
                0x79,
            );
            tx.send((top.0 as isize, GetCurrentThreadId())).unwrap();
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    });
    rx.recv().unwrap()
}

/// Bring the test window to the front: press the window's own registered hotkey, so the
/// window thread receives `WM_HOTKEY` and may call SetForegroundWindow (no other app sees the
/// chord while it is registered).
fn to_front(top: isize, _thread: u32) -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput, VIRTUAL_KEY,
    };
    let key = |vk: u16, up: bool| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: 0,
                dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let seq = [
        key(0x11, false),
        key(0x12, false),
        key(0x10, false),
        key(0x79, false),
        key(0x79, true),
        key(0x10, true),
        key(0x12, true),
        key(0x11, true),
    ];
    // SAFETY: well-formed keyboard inputs; every key pressed is released.
    unsafe {
        SendInput(&seq, size_of::<INPUT>() as i32);
    }
    std::thread::sleep(Duration::from_millis(300));
    // SAFETY: plain query.
    unsafe { GetForegroundWindow().0 as isize == top }
}

fn post(top: isize, m: u32) {
    // SAFETY: posting a private message to the test's own window.
    unsafe {
        let _ = PostMessageW(Some(HWND(top as *mut _)), m, WPARAM(0), LPARAM(0));
    }
    std::thread::sleep(Duration::from_millis(250));
}

#[test]
#[ignore = "real desktop: creates a test window, uses the clipboard (text saved and restored)"]
fn insertion_goes_only_into_the_verified_editable_target_and_restores_the_clipboard() {
    let (top, thread) = spawn_window();
    if !to_front(top, thread) {
        eprintln!("SKIPPED: could not bring the test window to the front");
        return;
    }
    let uia = UiaReader::new().unwrap();

    // Normal field: insertable.
    post(top, FOCUS_NORMAL);
    let target = uia.focus().unwrap();
    println!(
        "REAL normal target: password={} editable={}",
        target.is_password, target.editable
    );
    assert!(target.insertable(), "{target:?}");
    assert_eq!(target.hwnd, top);

    // Password field: never insertable; and it is a different target.
    post(top, FOCUS_PASSWORD);
    let pw = uia.focus().unwrap();
    println!(
        "REAL password target: password={} editable={}",
        pw.is_password, pw.editable
    );
    assert!(pw.is_password && !pw.insertable());
    assert!(!pw.same_as(&target), "a changed target is detected");

    // Insert into the normal field after re-verifying it.
    post(top, FOCUS_NORMAL);
    let again = uia.focus().unwrap();
    assert!(again.same_as(&target) && again.insertable());
    let before = inspect().map(|(t, _)| t);
    let formats_before = snapshot();
    let draft = "Thanks Ravi, the sheet is attached. [tsr-insert-test]";
    let guard = match set_private_text(draft) {
        Ok(g) => g,
        Err(ClipError::NotText) => {
            eprintln!("SKIPPED: the clipboard holds non-text data, which would be lost");
            return;
        }
        Err(e) => panic!("{e}"),
    };
    let (on_clip, private) = inspect().unwrap();
    assert_eq!(on_clip.as_deref(), Some(draft));
    assert!(
        private,
        "history/cloud exclusion formats are on the clipboard with the draft"
    );
    send_paste().unwrap();
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(edit_text(NORMAL.load(Ordering::Acquire)), draft);
    assert!(
        edit_text(PASSWORD.load(Ordering::Acquire)).is_empty(),
        "nothing typed into the password field"
    );
    std::thread::sleep(Duration::from_millis(500));
    assert!(restore(guard).unwrap(), "restored");
    let after = inspect().unwrap().0;
    assert_eq!(after, before.unwrap(), "previous clipboard text is back");
    assert_eq!(
        snapshot().unwrap(),
        formats_before.unwrap(),
        "every saved format is back, same sizes"
    );
    println!("REAL insertion ok; clipboard restored");
    // SAFETY: closing the test's own window.
    unsafe {
        let _ = PostMessageW(Some(HWND(top as *mut _)), WM_CLOSE, WPARAM(0), LPARAM(0));
    }
}
