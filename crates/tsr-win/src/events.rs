//! The T1 OS-events thread (plan §4.1, §5.1, §5.2).
//!
//! One thread owns a Windows message loop and delivers [`OsEvent`]s over a channel:
//! - `EVENT_SYSTEM_FOREGROUND` for focus changes (out-of-context WinEvent hook, all processes);
//! - `EVENT_OBJECT_NAMECHANGE` for title changes — hooked **only for the current foreground
//!   process** and re-registered on every focus change, because a system-wide name-change hook
//!   fires for every renamed control in every app;
//! - `WM_WTSSESSION_CHANGE` (lock/unlock), `WM_POWERBROADCAST` (suspend/resume, AC/DC source,
//!   battery saver) on a hidden message-only window.
//!
//! Tessara's own windows are ignored. Events carry handles and ids only, never titles.

use std::cell::{Cell, RefCell};
use std::sync::mpsc::Sender;

use windows::Win32::Foundation::{HANDLE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Power::{POWERBROADCAST_SETTING, RegisterPowerSettingNotification};
use windows::Win32::System::RemoteDesktop::WTSRegisterSessionNotification;
use windows::Win32::System::SystemServices::{GUID_ACDC_POWER_SOURCE, GUID_POWER_SAVING_STATUS};
use windows::Win32::System::Threading::{GetCurrentProcessId, GetCurrentThreadId};
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::w;

use crate::WinError;

/// Window handles cross threads as plain integers.
pub type Hwnd = isize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OsEvent {
    Foreground { hwnd: Hwnd, pid: u32 },
    TitleChanged { hwnd: Hwnd },
    SessionLocked,
    SessionUnlocked,
    Suspending,
    Resumed,
    PowerSource { on_ac: bool },
    BatterySaver { on: bool },
}

const WM_WTSSESSION_CHANGE_: u32 = 0x02B1;
const WTS_SESSION_LOCK_: usize = 0x7;
const WTS_SESSION_UNLOCK_: usize = 0x8;
const WM_POWERBROADCAST_: u32 = 0x0218;
const PBT_APMSUSPEND_: usize = 0x4;
const PBT_APMRESUMEAUTOMATIC_: usize = 0x12;
const PBT_POWERSETTINGCHANGE_: usize = 0x8013;
const NOTIFY_FOR_THIS_SESSION_: u32 = 0;
const EVENT_SYSTEM_FOREGROUND_: u32 = 0x0003;
const EVENT_OBJECT_NAMECHANGE_: u32 = 0x800C;
const WINEVENT_OUTOFCONTEXT_: u32 = 0x0000;
const WINEVENT_SKIPOWNPROCESS_: u32 = 0x0002;

thread_local! {
    static SENDER: RefCell<Option<Sender<OsEvent>>> = const { RefCell::new(None) };
    static NAME_HOOK: Cell<Option<isize>> = const { Cell::new(None) };
    static FOREGROUND: Cell<isize> = const { Cell::new(0) };
}

fn send(e: OsEvent) {
    SENDER.with(|s| {
        if let Some(tx) = s.borrow().as_ref() {
            let _ = tx.send(e);
        }
    });
}

fn hook_name_changes(pid: u32) {
    NAME_HOOK.with(|h| {
        if let Some(old) = h.take() {
            // SAFETY: a hook handle we created on this thread.
            unsafe {
                let _ = UnhookWinEvent(HWINEVENTHOOK(old as *mut _));
            }
        }
        // SAFETY: callback is a plain fn; out-of-context hooks run on this thread's message loop.
        let new = unsafe {
            SetWinEventHook(
                EVENT_OBJECT_NAMECHANGE_,
                EVENT_OBJECT_NAMECHANGE_,
                None,
                Some(on_win_event),
                pid,
                0,
                WINEVENT_OUTOFCONTEXT_ | WINEVENT_SKIPOWNPROCESS_,
            )
        };
        if !new.is_invalid() {
            h.set(Some(new.0 as isize));
        }
    });
}

unsafe extern "system" fn on_win_event(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    id_child: i32,
    _thread: u32,
    _time: u32,
) {
    if hwnd.is_invalid() {
        return;
    }
    match event {
        EVENT_SYSTEM_FOREGROUND_ => {
            let mut pid = 0u32;
            // SAFETY: read-only query on a handle Windows just gave us.
            unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
            if pid == 0 || pid == unsafe { GetCurrentProcessId() } {
                return;
            }
            FOREGROUND.with(|f| f.set(hwnd.0 as isize));
            hook_name_changes(pid);
            send(OsEvent::Foreground {
                hwnd: hwnd.0 as isize,
                pid,
            });
        }
        // Only the foreground window's own title, not its controls.
        EVENT_OBJECT_NAMECHANGE_
            if id_object == OBJID_WINDOW.0
                && id_child == CHILDID_SELF as i32
                && FOREGROUND.with(|f| f.get()) == hwnd.0 as isize =>
        {
            send(OsEvent::TitleChanged { hwnd: hwnd.0 as isize });
        }
        _ => {}
    }
}

fn read_u32(setting: &POWERBROADCAST_SETTING) -> Option<u32> {
    if setting.DataLength < 4 {
        return None;
    }
    // SAFETY: Data is a variable-length trailing array of DataLength bytes.
    Some(unsafe { std::ptr::read_unaligned(setting.Data.as_ptr() as *const u32) })
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_WTSSESSION_CHANGE_ => match wparam.0 {
            WTS_SESSION_LOCK_ => send(OsEvent::SessionLocked),
            WTS_SESSION_UNLOCK_ => send(OsEvent::SessionUnlocked),
            _ => {}
        },
        WM_POWERBROADCAST_ => match wparam.0 {
            PBT_APMSUSPEND_ => send(OsEvent::Suspending),
            PBT_APMRESUMEAUTOMATIC_ => send(OsEvent::Resumed),
            PBT_POWERSETTINGCHANGE_ if lparam.0 != 0 => {
                // SAFETY: for PBT_POWERSETTINGCHANGE, lparam points to a POWERBROADCAST_SETTING.
                let s = unsafe { &*(lparam.0 as *const POWERBROADCAST_SETTING) };
                if s.PowerSetting == GUID_ACDC_POWER_SOURCE {
                    if let Some(v) = read_u32(s) {
                        send(OsEvent::PowerSource { on_ac: v == 0 });
                    }
                } else if s.PowerSetting == GUID_POWER_SAVING_STATUS
                    && let Some(v) = read_u32(s)
                {
                    send(OsEvent::BatterySaver { on: v != 0 });
                }
            }
            _ => {}
        },
        _ => {}
    }
    // SAFETY: default handling for everything else.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// Handle to the running T1 thread. Dropping it stops the thread.
pub struct OsEventThread {
    thread_id: u32,
    join: Option<std::thread::JoinHandle<()>>,
}

impl OsEventThread {
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        if let Some(j) = self.join.take() {
            // SAFETY: posting WM_QUIT to a thread we started.
            unsafe {
                let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
            }
            let _ = j.join();
        }
    }
}

impl Drop for OsEventThread {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Start T1. Returns once hooks and the notification window are registered.
pub fn spawn(tx: Sender<OsEvent>) -> Result<OsEventThread, WinError> {
    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<u32, WinError>>();
    let join = std::thread::Builder::new()
        .name("tsr-os-events".into())
        .spawn(move || {
            SENDER.with(|s| *s.borrow_mut() = Some(tx));
            // SAFETY: all calls below set up and run this thread's own message loop.
            unsafe {
                let fg = SetWinEventHook(
                    EVENT_SYSTEM_FOREGROUND_,
                    EVENT_SYSTEM_FOREGROUND_,
                    None,
                    Some(on_win_event),
                    0,
                    0,
                    WINEVENT_OUTOFCONTEXT_ | WINEVENT_SKIPOWNPROCESS_,
                );
                if fg.is_invalid() {
                    let _ = ready_tx.send(Err(WinError {
                        op: "SetWinEventHook",
                        code: 0,
                    }));
                    return;
                }
                let hinst = GetModuleHandleW(None).unwrap_or_default();
                let class = w!("TessaraOsEvents");
                let wc = WNDCLASSW {
                    lpfnWndProc: Some(wndproc),
                    hInstance: hinst.into(),
                    lpszClassName: class,
                    ..Default::default()
                };
                RegisterClassW(&wc);
                let hwnd = match CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    class,
                    w!(""),
                    WINDOW_STYLE(0),
                    0,
                    0,
                    0,
                    0,
                    Some(HWND_MESSAGE),
                    None,
                    Some(hinst.into()),
                    None,
                ) {
                    Ok(h) => h,
                    Err(e) => {
                        let _ = ready_tx.send(Err(WinError {
                            op: "CreateWindowExW",
                            code: e.code().0,
                        }));
                        return;
                    }
                };
                let _ = WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION_);
                let h = HANDLE(hwnd.0);
                let _ = RegisterPowerSettingNotification(h, &GUID_ACDC_POWER_SOURCE, DEVICE_NOTIFY_WINDOW_HANDLE);
                let _ = RegisterPowerSettingNotification(h, &GUID_POWER_SAVING_STATUS, DEVICE_NOTIFY_WINDOW_HANDLE);

                // Report the window that already has focus, so consumers need not wait for a change.
                let cur = GetForegroundWindow();
                if !cur.is_invalid() {
                    let mut pid = 0u32;
                    GetWindowThreadProcessId(cur, Some(&mut pid));
                    if pid != 0 && pid != GetCurrentProcessId() {
                        FOREGROUND.with(|f| f.set(cur.0 as isize));
                        hook_name_changes(pid);
                        send(OsEvent::Foreground {
                            hwnd: cur.0 as isize,
                            pid,
                        });
                    }
                }
                let _ = ready_tx.send(Ok(GetCurrentThreadId()));

                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                NAME_HOOK.with(|h| {
                    if let Some(old) = h.take() {
                        let _ = UnhookWinEvent(HWINEVENTHOOK(old as *mut _));
                    }
                });
                let _ = UnhookWinEvent(fg);
                let _ = DestroyWindow(hwnd);
            }
        })
        .map_err(|_| WinError { op: "spawn", code: 0 })?;
    match ready_rx.recv() {
        Ok(Ok(thread_id)) => Ok(OsEventThread {
            thread_id,
            join: Some(join),
        }),
        Ok(Err(e)) => Err(e),
        Err(_) => Err(WinError {
            op: "os-events thread",
            code: 0,
        }),
    }
}
