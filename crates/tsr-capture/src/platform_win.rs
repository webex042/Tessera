//! Windows platform for the capture engine, and the capture thread (plan §4.1, §4.2).

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tsr_core::Sensitive;
use tsr_win::events::OsEvent;
use tsr_win::query::{self, UserState};
use windows::Win32::Foundation::HWND;

use crate::deadline::DeadlineError;
use crate::engine::{BrowserInfo, CaptureCore, Control, Foreground, OsState, Platform, ReadOutcome, Signal, Sink};
use crate::gate::UserActivity;
use crate::ocr::OcrReader;
use crate::uia::{UiaError, UiaReader};

pub struct WinPlatform {
    uia: UiaReader,
    ocr: OcrReader,
    own_pid: u32,
}

impl WinPlatform {
    pub fn new() -> std::io::Result<Self> {
        Ok(WinPlatform {
            uia: UiaReader::new()?,
            ocr: OcrReader::new()?,
            own_pid: std::process::id(),
        })
    }
}

impl Platform for WinPlatform {
    fn foreground(&mut self) -> Option<Foreground> {
        let f = query::foreground()?;
        if f.pid == self.own_pid {
            return None; // never capture Tessara's own windows
        }
        Some(Foreground {
            hwnd: f.hwnd,
            pid: f.pid,
            exe: f.exe,
            exe_path: f.exe_path,
            title: Sensitive::new(f.title),
            minimized: f.minimized,
        })
    }

    fn os_state(&mut self) -> OsState {
        OsState {
            idle_ms: query::idle_ms(),
            activity: match query::user_state() {
                UserState::Normal | UserState::Quiet => UserActivity::Normal,
                UserState::FullscreenD3D => UserActivity::Fullscreen,
                UserState::Presentation => UserActivity::Presentation,
                UserState::Busy => UserActivity::Busy,
                UserState::NotPresent => UserActivity::NotPresent,
                UserState::Unknown => UserActivity::Unknown,
            },
        }
    }

    fn browser(&mut self, w: &Foreground) -> Option<BrowserInfo> {
        self.uia.browser(HWND(w.hwnd as *mut _)).ok().map(|b| BrowserInfo {
            url: b.url,
            private: b.private,
        })
    }

    fn read_uia(&mut self, w: &Foreground) -> ReadOutcome {
        match self.uia.read(HWND(w.hwnd as *mut _)) {
            Ok(t) => ReadOutcome::Text(Sensitive::new(t.text)),
            Err(UiaError::Deadline(DeadlineError::TimedOut)) => ReadOutcome::TimedOut,
            Err(_) => ReadOutcome::Failed,
        }
    }

    fn read_ocr(&mut self, w: &Foreground) -> ReadOutcome {
        match self.ocr.read(HWND(w.hwnd as *mut _)) {
            Ok(o) => ReadOutcome::Text(o.text),
            Err(_) => ReadOutcome::Failed,
        }
    }
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Controls a running capture thread.
pub struct CaptureHandle<S: Sink + Send + 'static> {
    ctl: Sender<Control>,
    join: Option<JoinHandle<CaptureCore<WinPlatform, S>>>,
}

impl<S: Sink + Send + 'static> CaptureHandle<S> {
    pub fn send(&self, c: Control) {
        let _ = self.ctl.send(c);
    }

    pub fn pause(&self) {
        self.send(Control::Pause);
    }

    pub fn resume(&self) {
        self.send(Control::Resume);
    }

    /// Flush open sessions, stop, and hand back the engine (with its sink and stats).
    pub fn shutdown(mut self) -> CaptureCore<WinPlatform, S> {
        self.send(Control::Shutdown);
        self.join
            .take()
            .expect("capture thread")
            .join()
            .expect("capture thread panicked")
    }
}

/// Capture-relevant OS events; display, settings and hotkey events belong to the UI.
fn to_signal(e: OsEvent) -> Option<Signal> {
    Some(match e {
        OsEvent::Foreground { .. } => Signal::ForegroundChanged,
        OsEvent::TitleChanged { .. } => Signal::TitleChanged,
        OsEvent::SessionLocked => Signal::Locked,
        OsEvent::SessionUnlocked => Signal::Unlocked,
        OsEvent::Suspending => Signal::Suspending,
        OsEvent::Resumed => Signal::Resumed,
        OsEvent::PowerSource { on_ac } => Signal::PowerSource { on_ac },
        OsEvent::BatterySaver { on } => Signal::BatterySaver { on },
        OsEvent::DisplayChanged
        | OsEvent::SettingsChanged
        | OsEvent::Hotkey { .. }
        | OsEvent::HotkeyUnavailable { .. }
        | OsEvent::HotkeyRegistered { .. } => {
            return None;
        }
    })
}

/// Start the capture thread and the OS-events thread feeding it. Capture starts **paused**.
/// The engine must be built on the capture thread (UIA/OCR workers are tied to their threads),
/// so pass a constructor.
pub fn spawn_capture<S, F>(make: F) -> std::io::Result<CaptureHandle<S>>
where
    S: Sink + Send + 'static,
    F: FnOnce() -> std::io::Result<CaptureCore<WinPlatform, S>> + Send + 'static,
{
    let (ctl, rx): (Sender<Control>, Receiver<Control>) = mpsc::channel();
    let (os_tx, os_rx) = mpsc::channel::<OsEvent>();
    let fwd = ctl.clone();
    std::thread::Builder::new()
        .name("tsr-os-forward".into())
        .spawn(move || {
            for e in os_rx {
                let Some(sig) = to_signal(e) else { continue };
                if fwd.send(Control::Os(sig)).is_err() {
                    break;
                }
            }
        })?;
    let (ready_tx, ready_rx) = mpsc::channel::<std::io::Result<()>>();
    let join = std::thread::Builder::new().name("tsr-capture".into()).spawn(move || {
        let mut core = match make() {
            Ok(c) => c,
            Err(e) => {
                let _ = ready_tx.send(Err(e));
                panic!("capture engine could not start");
            }
        };
        let events = tsr_win::events::spawn(os_tx).ok();
        let p = query::power();
        if let Some(on_ac) = p.on_ac {
            core.handle(Control::Os(Signal::PowerSource { on_ac }), now_ms());
        }
        core.handle(Control::Os(Signal::BatterySaver { on: p.battery_saver }), now_ms());
        let _ = ready_tx.send(Ok(()));
        loop {
            match rx.recv_timeout(Duration::from_millis(250)) {
                Ok(c) => {
                    if !core.handle(c, now_ms()) {
                        break;
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    core.handle(Control::Shutdown, now_ms());
                    break;
                }
            }
            core.tick(now_ms());
        }
        if let Some(t) = events {
            t.stop();
        }
        core
    })?;
    ready_rx
        .recv()
        .map_err(|_| std::io::Error::other("capture thread failed to start"))??;
    Ok(CaptureHandle { ctl, join: Some(join) })
}
