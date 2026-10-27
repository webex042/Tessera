//! Safe wrappers over the Windows APIs Tessara uses (plan §3 `tsr-win`).
//!
//! So far: DPAPI key protection and the OS random generator ([`dpapi`]). WinEvent hooks, WTS,
//! power notifications, Job Objects and display affinity follow in later Stage 1 work.

#[cfg(windows)]
pub mod dpapi;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("{op} failed with {code:#010x}")]
pub struct WinError {
    pub op: &'static str,
    pub code: i32,
}
