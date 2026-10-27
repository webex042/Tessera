//! Capture subsystem (plan §5).
//!
//! - [`engine`]: the capture loop logic (gates → blocklist → rung 1/2 → redaction → sessions).
//! - [`gate`], [`session`], [`simhash`], [`fallback`], [`ocr_guard`]: pure, platform-neutral parts.
//! - [`uia`], [`ocr`] (Windows): rung 1 and rung 2 readers with wall-clock deadlines
//!   ([`deadline`]); `platform_win` and `sink` wire them to the OS and the store.

pub mod deadline;
pub mod engine;
pub mod fallback;
pub mod gate;
#[cfg(windows)]
pub mod ocr;
pub mod ocr_guard;
#[cfg(windows)]
pub mod platform_win;
pub mod session;
pub mod simhash;
pub mod sink;
#[cfg(windows)]
pub mod uia;
