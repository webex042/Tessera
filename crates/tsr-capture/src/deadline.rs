//! A worker thread whose calls return within a wall-clock deadline (plan §5.4 rung 1, §19).
//!
//! Stage 0 found that UIA's own connection/transaction timeouts are per call, not per read: Word
//! took 2.8 s to fail with 300 ms timeouts set. So reads run on a dedicated thread (plan §4.1 T2),
//! and the caller waits at most `deadline`. If the worker is still busy when the deadline passes,
//! the caller gets [`DeadlineError::TimedOut`] and the late result is discarded when it arrives.
//! While the worker is stuck, new calls return [`DeadlineError::Busy`] at once instead of queueing
//! behind it, so one hung app can never delay capture of the next window.
//!
//! State that must live on the worker thread (COM objects are not `Send`) is built there by `init`.

use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DeadlineError {
    #[error("deadline passed; result will be discarded")]
    TimedOut,
    #[error("worker still busy with an earlier call")]
    Busy,
    #[error("worker failed (panicked or stopped)")]
    Failed,
}

struct Job<Req, Resp> {
    req: Req,
    reply: mpsc::Sender<Resp>,
}

pub struct DeadlineWorker<Req, Resp> {
    tx: mpsc::Sender<Job<Req, Resp>>,
    busy: Arc<AtomicBool>,
}

impl<Req: Send + 'static, Resp: Send + 'static> DeadlineWorker<Req, Resp> {
    /// Start the worker. `init` runs on the new thread and builds its state; `handler` serves one
    /// request at a time. A panic in `handler` fails that call only; the worker keeps running.
    pub fn spawn<S, I, H>(name: &str, init: I, mut handler: H) -> std::io::Result<Self>
    where
        I: FnOnce() -> S + Send + 'static,
        H: FnMut(&mut S, Req) -> Resp + Send + 'static,
    {
        let (tx, rx) = mpsc::channel::<Job<Req, Resp>>();
        let (ready_tx, ready_rx) = mpsc::channel::<()>();
        let busy = Arc::new(AtomicBool::new(false));
        let flag = busy.clone();
        std::thread::Builder::new().name(name.to_owned()).spawn(move || {
            let mut state = init();
            let _ = ready_tx.send(());
            while let Ok(job) = rx.recv() {
                let out = std::panic::catch_unwind(AssertUnwindSafe(|| handler(&mut state, job.req)));
                // Mark idle *before* replying, so a caller that just got its answer can
                // immediately make the next call without seeing a stale "busy".
                flag.store(false, Ordering::Release);
                if let Ok(resp) = out {
                    let _ = job.reply.send(resp); // receiver gone = caller timed out; discard
                }
            }
        })?;
        // Initialisation (e.g. creating the UIA client) must not eat into the first call's
        // deadline, so wait for it here.
        ready_rx
            .recv_timeout(Duration::from_secs(10))
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "worker initialisation"))?;
        Ok(DeadlineWorker { tx, busy })
    }

    /// Run `req` on the worker and wait at most `deadline` for the answer.
    pub fn call(&self, req: Req, deadline: Duration) -> Result<Resp, DeadlineError> {
        if self
            .busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(DeadlineError::Busy);
        }
        let (reply, rx) = mpsc::channel();
        if self.tx.send(Job { req, reply }).is_err() {
            self.busy.store(false, Ordering::Release);
            return Err(DeadlineError::Failed);
        }
        match rx.recv_timeout(deadline) {
            Ok(r) => Ok(r),
            Err(RecvTimeoutError::Timeout) => Err(DeadlineError::TimedOut),
            Err(RecvTimeoutError::Disconnected) => Err(DeadlineError::Failed),
        }
    }

    pub fn is_busy(&self) -> bool {
        self.busy.load(Ordering::Acquire)
    }

    /// Wait until the worker has finished an abandoned call (for diagnostics and tests).
    /// Returns how long that took, or `None` if it is still busy after `max`.
    pub fn wait_idle(&self, max: Duration) -> Option<Duration> {
        let t0 = Instant::now();
        while self.is_busy() {
            if t0.elapsed() > max {
                return None;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        Some(t0.elapsed())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sleeper() -> DeadlineWorker<u64, u64> {
        // Request = how long to "work" in ms; response = that number.
        DeadlineWorker::spawn(
            "test-worker",
            || (),
            |_, ms| {
                std::thread::sleep(Duration::from_millis(ms));
                ms
            },
        )
        .unwrap()
    }

    #[test]
    fn fast_call_returns_its_result() {
        let w = sleeper();
        assert_eq!(w.call(5, Duration::from_millis(300)), Ok(5));
    }

    #[test]
    fn slow_call_returns_at_the_deadline_then_worker_is_busy() {
        let w = sleeper();
        let t0 = Instant::now();
        assert_eq!(w.call(2_000, Duration::from_millis(300)), Err(DeadlineError::TimedOut));
        let waited = t0.elapsed();
        assert!(
            waited >= Duration::from_millis(300) && waited < Duration::from_millis(450),
            "{waited:?}"
        );

        // The stuck call blocks nothing: the next call fails fast instead of queueing.
        let t1 = Instant::now();
        assert_eq!(w.call(1, Duration::from_millis(300)), Err(DeadlineError::Busy));
        assert!(t1.elapsed() < Duration::from_millis(20));

        // Once the abandoned call finishes, the worker serves again; its late result is dropped.
        assert!(w.wait_idle(Duration::from_secs(5)).is_some());
        assert_eq!(w.call(3, Duration::from_millis(300)), Ok(3));
    }

    #[test]
    fn back_to_back_calls_never_see_a_stale_busy() {
        let w = sleeper();
        for _ in 0..2_000 {
            assert_eq!(w.call(0, Duration::from_millis(300)), Ok(0));
        }
    }

    #[test]
    fn handler_panic_fails_one_call_only() {
        let w: DeadlineWorker<u32, u32> = DeadlineWorker::spawn(
            "panicky",
            || (),
            |_, x| {
                if x == 13 {
                    panic!("boom");
                }
                x
            },
        )
        .unwrap();
        assert_eq!(w.call(13, Duration::from_millis(300)), Err(DeadlineError::Failed));
        assert_eq!(w.call(7, Duration::from_millis(300)), Ok(7));
    }

    #[test]
    fn slow_initialisation_does_not_count_against_the_first_call() {
        let w: DeadlineWorker<(), ()> = DeadlineWorker::spawn(
            "slow-init",
            || std::thread::sleep(Duration::from_millis(400)),
            |_, ()| (),
        )
        .unwrap();
        assert_eq!(w.call((), Duration::from_millis(100)), Ok(()));
    }

    #[test]
    fn state_lives_on_the_worker_thread() {
        let w: DeadlineWorker<(), String> = DeadlineWorker::spawn(
            "named",
            || std::thread::current().name().unwrap_or("").to_owned(),
            |s, ()| s.clone(),
        )
        .unwrap();
        assert_eq!(w.call((), Duration::from_millis(300)).unwrap(), "named");
    }
}
