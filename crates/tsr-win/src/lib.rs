//! Safe wrappers over the Windows APIs Tessara uses (plan §3 `tsr-win`, §4.1 T1, §5.1, §5.2).
//!
//! - [`display`]: capture exclusion for Tessara's own windows.
//! - [`dpapi`]: key protection and OS randomness.
//! - [`events`]: the T1 thread — WinEvent hooks (foreground, title), session lock/unlock,
//!   suspend/resume, AC/battery and battery-saver notifications.
//! - [`query`]: point-in-time OS state — foreground window facts, idle time, fullscreen /
//!   presentation state, power status.
//!
//! No low-level keyboard or mouse hooks are used anywhere (plan D11).

#[cfg(windows)]
pub mod clipboard;
#[cfg(windows)]
pub mod display;
#[cfg(windows)]
pub mod dpapi;
#[cfg(windows)]
pub mod events;
#[cfg(windows)]
pub mod job;
#[cfg(windows)]
pub mod query;
#[cfg(windows)]
pub mod tcp;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("{op} failed with {code:#010x}")]
pub struct WinError {
    pub op: &'static str,
    pub code: i32,
}
