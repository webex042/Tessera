//! Tessara core types (plan §3, §6.6).
//!
//! - [`Sensitive`]: wraps anything that came from the screen or the user. Its `Debug` and
//!   `Display` print `<redacted>`, so it cannot leak through `{:?}`, `%`, panics or error chains.
//! - [`logging`]: a `tracing` layer that writes only allowlisted, content-free fields.

pub mod logging;
pub mod sensitive;

pub use sensitive::Sensitive;
