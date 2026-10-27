//! Cancellation shared between the scheduler, the sidecar supervisor and in-flight requests.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum CancelReason {
    /// A higher-priority job needs the model; the job is re-queued (plan §7.4).
    Preempted,
    /// The user or the caller gave up on this job.
    Cancelled,
    /// The app is shutting down.
    Shutdown,
}

impl CancelReason {
    fn code(self) -> u8 {
        match self {
            CancelReason::Preempted => 1,
            CancelReason::Cancelled => 2,
            CancelReason::Shutdown => 3,
        }
    }

    fn from_code(c: u8) -> Option<Self> {
        match c {
            1 => Some(CancelReason::Preempted),
            2 => Some(CancelReason::Cancelled),
            3 => Some(CancelReason::Shutdown),
            _ => None,
        }
    }
}

/// Cheap to clone; every clone sees the same state. The first reason set wins, so a job that was
/// preempted and then shut down reports `Preempted` (and is re-queued in the database on restart).
#[derive(Clone, Debug, Default)]
pub struct CancelToken(Arc<AtomicU8>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns `true` if this call set the reason.
    pub fn cancel(&self, reason: CancelReason) -> bool {
        self.0
            .compare_exchange(0, reason.code(), Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    pub fn reason(&self) -> Option<CancelReason> {
        CancelReason::from_code(self.0.load(Ordering::Acquire))
    }

    pub fn is_cancelled(&self) -> bool {
        self.reason().is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_reason_wins_and_clones_share_state() {
        let t = CancelToken::new();
        let c = t.clone();
        assert!(!t.is_cancelled());
        assert!(c.cancel(CancelReason::Preempted));
        assert!(!t.cancel(CancelReason::Shutdown));
        assert_eq!(t.reason(), Some(CancelReason::Preempted));
    }
}
