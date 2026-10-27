//! Point-in-time OS state for the capture gates (plan §5.1, §5.2).

use windows::Win32::Foundation::{CloseHandle, HWND};
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::System::SystemInformation::{GetTickCount, GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
use windows::Win32::UI::Shell::{
    QUNS_ACCEPTS_NOTIFICATIONS, QUNS_APP, QUNS_BUSY, QUNS_NOT_PRESENT, QUNS_PRESENTATION_MODE, QUNS_QUIET_TIME,
    QUNS_RUNNING_D3D_FULL_SCREEN, SHQueryUserNotificationState,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClassNameW, GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsIconic,
};
use windows::core::PWSTR;

use crate::events::Hwnd;

/// Facts about a window. `title` is screen content: callers redact it before storing.
#[derive(Clone, PartialEq, Eq)]
pub struct WindowFacts {
    pub hwnd: Hwnd,
    pub pid: u32,
    /// Image file name, e.g. `chrome.exe`.
    pub exe: String,
    pub exe_path: String,
    pub title: String,
    pub class: String,
    pub minimized: bool,
}

impl std::fmt::Debug for WindowFacts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // No title: it is content.
        f.debug_struct("WindowFacts")
            .field("pid", &self.pid)
            .field("exe", &self.exe)
            .field("class", &self.class)
            .finish()
    }
}

/// Process image path for a pid (`QueryFullProcessImageNameW`).
pub fn process_image(pid: u32) -> Option<String> {
    // SAFETY: limited-information handle, closed before returning.
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
        let _ = CloseHandle(h);
        ok.then(|| String::from_utf16_lossy(&buf[..len as usize]))
    }
}

pub fn window_facts(hwnd: Hwnd) -> Option<WindowFacts> {
    let h = HWND(hwnd as *mut _);
    // SAFETY: read-only queries on a window handle.
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(h, Some(&mut pid));
        if pid == 0 {
            return None;
        }
        let exe_path = process_image(pid)?;
        let exe = exe_path.rsplit('\\').next().unwrap_or("").to_owned();
        let n = GetWindowTextLengthW(h).max(0) as usize;
        let mut t = vec![0u16; n + 1];
        let got = GetWindowTextW(h, &mut t) as usize;
        let mut c = [0u16; 256];
        let cl = GetClassNameW(h, &mut c) as usize;
        Some(WindowFacts {
            hwnd,
            pid,
            exe,
            exe_path,
            title: String::from_utf16_lossy(&t[..got]),
            class: String::from_utf16_lossy(&c[..cl]),
            minimized: IsIconic(h).as_bool(),
        })
    }
}

pub fn foreground() -> Option<WindowFacts> {
    // SAFETY: plain query.
    let h = unsafe { GetForegroundWindow() };
    if h.is_invalid() {
        None
    } else {
        window_facts(h.0 as isize)
    }
}

/// Milliseconds since the last keyboard/mouse input in this session.
pub fn idle_ms() -> u64 {
    let mut lii = LASTINPUTINFO {
        cbSize: size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    // SAFETY: out-param struct with cbSize set.
    unsafe {
        if !GetLastInputInfo(&mut lii).as_bool() {
            return 0;
        }
        // Both are 32-bit tick counts; wrapping subtraction handles the 49-day rollover.
        GetTickCount().wrapping_sub(lii.dwTime) as u64
    }
}

/// What `SHQueryUserNotificationState` says the user is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserState {
    /// Normal desktop use.
    Normal,
    /// A full-screen D3D app (games, video).
    FullscreenD3D,
    /// Presentation mode is on.
    Presentation,
    /// Full-screen app or presentation settings ("busy").
    Busy,
    /// Screen saver, lock screen or fast user switching.
    NotPresent,
    /// The first hour after setup, or a Windows Store app is in front.
    Quiet,
    Unknown,
}

pub fn user_state() -> UserState {
    // SAFETY: plain query.
    match unsafe { SHQueryUserNotificationState() } {
        Ok(s) if s == QUNS_ACCEPTS_NOTIFICATIONS => UserState::Normal,
        Ok(s) if s == QUNS_RUNNING_D3D_FULL_SCREEN => UserState::FullscreenD3D,
        Ok(s) if s == QUNS_PRESENTATION_MODE => UserState::Presentation,
        Ok(s) if s == QUNS_BUSY => UserState::Busy,
        Ok(s) if s == QUNS_NOT_PRESENT => UserState::NotPresent,
        Ok(s) if s == QUNS_QUIET_TIME || s == QUNS_APP => UserState::Quiet,
        _ => UserState::Unknown,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PowerState {
    /// `None` when Windows does not know (e.g. desktops report "unknown" on some firmware).
    pub on_ac: Option<bool>,
    pub battery_saver: bool,
    pub battery_percent: Option<u8>,
}

pub fn power() -> PowerState {
    let mut s = SYSTEM_POWER_STATUS::default();
    // SAFETY: out-param struct.
    if unsafe { GetSystemPowerStatus(&mut s) }.is_err() {
        return PowerState {
            on_ac: None,
            battery_saver: false,
            battery_percent: None,
        };
    }
    PowerState {
        on_ac: match s.ACLineStatus {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        },
        battery_saver: s.SystemStatusFlag & 1 == 1,
        battery_percent: (s.BatteryLifePercent <= 100).then_some(s.BatteryLifePercent),
    }
}

/// Physical memory in MiB (`GlobalMemoryStatusEx`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryInfo {
    pub total_mib: u64,
    pub available_mib: u64,
}

/// Working set and CPU time of a process (the sidecar), for the diagnostics page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessUsage {
    pub working_set_mib: u64,
    pub cpu_ms: u64,
}

pub fn process_usage(pid: u32) -> Option<ProcessUsage> {
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows::Win32::System::Threading::{GetProcessTimes, PROCESS_VM_READ};
    // SAFETY: limited query handle, closed before returning; out-params sized correctly.
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ, false, pid)
            .or_else(|_| OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid))
            .ok()?;
        let mut pmc = PROCESS_MEMORY_COUNTERS {
            cb: size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
            ..Default::default()
        };
        let mem = GetProcessMemoryInfo(h, &mut pmc, pmc.cb).is_ok();
        let (mut c, mut e, mut k, mut u) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        let times = GetProcessTimes(h, &mut c, &mut e, &mut k, &mut u).is_ok();
        let _ = CloseHandle(h);
        if !mem || !times {
            return None;
        }
        let ft = |f: FILETIME| ((f.dwHighDateTime as u64) << 32 | f.dwLowDateTime as u64) / 10_000;
        Some(ProcessUsage {
            working_set_mib: (pmc.WorkingSetSize as u64) >> 20,
            cpu_ms: ft(k) + ft(u),
        })
    }
}

pub fn memory() -> Option<MemoryInfo> {
    let mut m = MEMORYSTATUSEX {
        dwLength: size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    // SAFETY: out-param struct with dwLength set.
    unsafe { GlobalMemoryStatusEx(&mut m) }.ok()?;
    Some(MemoryInfo {
        total_mib: m.ullTotalPhys >> 20,
        available_mib: m.ullAvailPhys >> 20,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// REAL: these query the machine running the tests; they assert only that the calls work.
    #[test]
    fn real_os_state_queries() {
        let fg = foreground();
        println!("REAL foreground: {fg:?}");
        println!("REAL idle_ms: {}", idle_ms());
        println!("REAL user_state: {:?}", user_state());
        println!("REAL power: {:?}", power());
        let mem = memory().expect("memory status");
        println!("REAL memory: {mem:?}");
        let me = process_usage(std::process::id()).expect("own usage");
        assert!(me.working_set_mib > 0);
        assert!(mem.total_mib > 0 && mem.available_mib <= mem.total_mib);
        let me = process_image(std::process::id()).expect("own image path");
        assert!(me.to_ascii_lowercase().ends_with(".exe"));
        assert!(!format!("{fg:?}").contains("title"), "Debug must not print titles");
    }
}
