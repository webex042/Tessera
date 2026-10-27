//! The only Tessara crate allowed to reach the internet (plan §3, §15.4, D10).
//!
//! It makes requests only for an explicit user action — today, downloading model files listed in
//! `tools/models.lock` — and only after the user approved that exact download ([`Approval`]).
//! `tools/check_network_deps.py` and `cargo deny` fail the build if any other crate can reach a
//! network-capable dependency.

pub mod download;

pub use download::{Approval, DownloadConfig, DownloadError, FileOutcome, Progress, download_model};
