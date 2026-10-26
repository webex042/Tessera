//! Capture subsystem (plan §5).
//!
//! Stage 1 so far: a wall-clock deadline for UI Automation reads ([`deadline`], [`uia`]) and the
//! per-window fallback to rung 2 when an app keeps timing out ([`fallback`]).

pub mod deadline;
pub mod fallback;
#[cfg(windows)]
pub mod uia;
