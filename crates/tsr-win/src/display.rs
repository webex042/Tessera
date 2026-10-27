//! Capture exclusion for Tessara's own windows (plan §13.1).
//!
//! `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)` asks the compositor to leave the window out
//! of screen captures (Windows 10 2004+). It is best-effort, not DRM: the setting is read back
//! with `GetWindowDisplayAffinity`, and whether captures really omit the window is verified on
//! pixels by `tools/verify-capture-exclusion.ps1` (`tsr-capture` binary `tsr-verify-exclusion`).

use windows::Win32::Foundation::{HWND, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{CreateRectRgn, GetWindowRgnBox, SetWindowRgn};
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetForegroundWindow, GetWindowDisplayAffinity, GetWindowThreadProcessId, IsWindow, IsWindowVisible,
    SetForegroundWindow, SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE, WDA_NONE,
};
use windows::core::BOOL;

use crate::WinError;

pub const AFFINITY_NONE: u32 = WDA_NONE.0;
pub const AFFINITY_EXCLUDE: u32 = WDA_EXCLUDEFROMCAPTURE.0;

pub fn affinity(hwnd: isize) -> Result<u32, WinError> {
    let mut a = 0u32;
    // SAFETY: out-param; hwnd is a window handle the caller owns.
    unsafe { GetWindowDisplayAffinity(HWND(hwnd as *mut _), &mut a) }.map_err(|e| WinError {
        op: "GetWindowDisplayAffinity",
        code: e.code().0,
    })?;
    Ok(a)
}

/// Exclude the window from captures and confirm the setting took. `Ok(true)` = read back as
/// excluded.
pub fn exclude_from_capture(hwnd: isize) -> Result<bool, WinError> {
    // SAFETY: plain call on a window owned by this process.
    unsafe { SetWindowDisplayAffinity(HWND(hwnd as *mut _), WDA_EXCLUDEFROMCAPTURE) }.map_err(|e| WinError {
        op: "SetWindowDisplayAffinity",
        code: e.code().0,
    })?;
    Ok(affinity(hwnd)? == AFFINITY_EXCLUDE)
}

/// Exclude every visible top-level window of this process (the notch, and helper windows the
/// UI toolkit creates, which draw nothing but are "visible"). Returns `(visible, excluded)`.
pub fn exclude_own_windows() -> (usize, usize) {
    unsafe extern "system" fn each(h: HWND, lp: LPARAM) -> BOOL {
        // SAFETY: lp is the &mut Vec passed below, alive for the EnumWindows call.
        let v = unsafe { &mut *(lp.0 as *mut Vec<isize>) };
        let mut pid = 0u32;
        // SAFETY: plain queries on a window handle from EnumWindows.
        unsafe { GetWindowThreadProcessId(h, Some(&mut pid)) };
        if pid == unsafe { GetCurrentProcessId() } && unsafe { IsWindowVisible(h) }.as_bool() {
            v.push(h.0 as isize);
        }
        BOOL(1)
    }
    let mut v: Vec<isize> = Vec::new();
    // SAFETY: the callback only uses the pointer to `v`, which outlives the call.
    let _ = unsafe { EnumWindows(Some(each), LPARAM(&mut v as *mut _ as isize)) };
    let ok = v.iter().filter(|h| exclude_from_capture(**h).unwrap_or(false)).count();
    (v.len(), ok)
}

/// Undo exclusion (only the capture-exclusion check uses this, to take its control capture).
pub fn include_in_capture(hwnd: isize) -> Result<(), WinError> {
    // SAFETY: plain call on a window owned by this process.
    unsafe { SetWindowDisplayAffinity(HWND(hwnd as *mut _), WDA_NONE) }.map_err(|e| WinError {
        op: "SetWindowDisplayAffinity",
        code: e.code().0,
    })
}

/// The foreground window if it belongs to another process (where focus should return when the
/// notch panel closes), else `None`.
pub fn foreign_foreground() -> Option<isize> {
    // SAFETY: plain queries.
    let h = unsafe { GetForegroundWindow() };
    if h.is_invalid() {
        return None;
    }
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(h, Some(&mut pid)) };
    (pid != unsafe { GetCurrentProcessId() }).then_some(h.0 as isize)
}

/// Give focus back to `hwnd` if it still exists. Allowed because the caller is the foreground
/// process at that moment (the user just used the notch).
pub fn restore_foreground(hwnd: isize) -> bool {
    let h = HWND(hwnd as *mut _);
    // SAFETY: IsWindow guards a stale handle; SetForegroundWindow is a plain request.
    unsafe { IsWindow(Some(h)).as_bool() && SetForegroundWindow(h).as_bool() }
}

/// Clip the window to a rectangle (window-relative physical pixels). Outside it the window is
/// neither drawn nor hit-tested, so clicks go to whatever is underneath. Changing the region is
/// far cheaper than resizing a WebView, which is why the notch keeps one window size and only
/// changes its region per panel (plan §13.2 latency budget).
pub fn set_region(hwnd: isize, x: i32, y: i32, w: i32, h: i32) -> bool {
    // SAFETY: the system takes ownership of the region on success; on failure nothing leaks
    // that matters (one GDI region).
    unsafe {
        let rgn = CreateRectRgn(x, y, x + w, y + h);
        SetWindowRgn(HWND(hwnd as *mut _), Some(rgn), true) != 0
    }
}

/// The window's region bounds (window-relative), if it has one.
pub fn region_box(hwnd: isize) -> Option<(i32, i32, i32, i32)> {
    let mut r = RECT::default();
    // SAFETY: out-param; returns 0 (ERROR) when the window has no region.
    let kind = unsafe { GetWindowRgnBox(HWND(hwnd as *mut _), &mut r) };
    (kind.0 > 1).then_some((r.left, r.top, r.right - r.left, r.bottom - r.top))
}
