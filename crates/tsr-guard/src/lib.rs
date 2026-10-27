//! Guardrails for Tessara (plan §6). Pure, platform-neutral code: everything here decides from
//! facts the capture layer hands in, so it can be tested without Windows.
//!
//! Stage 1 scope so far: window blocklists with per-rule scope (§6.1).

pub mod blocklist;
mod glob;
pub mod redact;
