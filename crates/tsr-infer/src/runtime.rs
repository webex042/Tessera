//! The threaded inference runtime: [`SchedulerCore`] + [`Supervisor`] + idle unload (plan §4.3,
//! §7.1, §7.4).
//!
//! Threads: one worker per [`Resource`] and one monitor. Workers take the next job, make sure the
//! sidecar is up (model jobs only — embedder jobs never start the model), run it through the
//! [`Runner`], and record the outcome. The monitor runs health checks, preempts running P1/P2
//! jobs when a P0 job is waiting, and unloads the model after the idle timeout.
//!
//! Race-free unload: the monitor marks the core `unloading` (model dispatch stops) *only* when no
//! model job is queued or running, then kills the sidecar, then clears the mark. A request that
//! arrives meanwhile waits in the queue and lazily restarts the sidecar afterwards.
//!
//! GPU work is only ever issued by a worker running a dispatched model job, so with an empty
//! queue the application issues no inference work at all (`model_requests_active == 0`); once
//! unloaded, no sidecar process exists.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::cancel::{CancelReason, CancelToken};
use crate::client::ChatModel;
use crate::metrics::{ProcessUsage, Snapshot};
use crate::sched::{
    Config as SchedConfig, Dispatch, Finish, Gates, JobId, JobSpec, Outcome, Resource, SchedulerCore, SubmitError,
};
use crate::supervisor::{Launch, State, Supervisor, Unavailable};

/// Runs one dispatched job. `model` is `Some` for model jobs. Must check `cancel` at safe
/// boundaries (the loopback client does so while streaming) and must not commit partial results:
/// return [`Outcome::Preempted`] / [`Outcome::Cancelled`] when the token fired.
pub trait Runner: Send {
    fn run(&mut self, job: &Dispatch, model: Option<&dyn ChatModel>, cancel: &CancelToken) -> Outcome;
}

/// When to unload the model after the last model job (plan §4.3: 10 min on battery, never on AC).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IdlePolicy {
    pub on_ac: Option<Duration>,
    pub on_battery: Option<Duration>,
}

impl Default for IdlePolicy {
    fn default() -> Self {
        IdlePolicy {
            on_ac: None,
            on_battery: Some(Duration::from_secs(600)),
        }
    }
}

impl IdlePolicy {
    pub fn timeout(&self, on_battery: bool) -> Option<Duration> {
        if on_battery { self.on_battery } else { self.on_ac }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Completion {
    pub id: JobId,
    pub key: String,
    pub outcome: Outcome,
}

struct Shared<L: Launch> {
    core: Mutex<SchedulerCore>,
    cv: Condvar,
    sup: Mutex<Supervisor<L>>,
    tokens: Mutex<HashMap<JobId, CancelToken>>,
    gates: Mutex<Gates>,
    on_battery: AtomicBool,
    idle: Mutex<IdlePolicy>,
    stop: AtomicBool,
    subscribers: Mutex<Vec<Sender<Completion>>>,
    model_active: AtomicU32,
    model_total: AtomicU64,
    embed_total: AtomicU64,
}

pub struct Runtime<L: Launch + 'static> {
    shared: Arc<Shared<L>>,
    threads: Vec<JoinHandle<()>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

impl<L: Launch + 'static> Runtime<L> {
    pub fn start(
        sched: SchedConfig,
        sup: Supervisor<L>,
        idle: IdlePolicy,
        model_runner: Box<dyn Runner>,
        embed_runner: Box<dyn Runner>,
    ) -> Self {
        let shared = Arc::new(Shared {
            core: Mutex::new(SchedulerCore::new(sched)),
            cv: Condvar::new(),
            sup: Mutex::new(sup),
            tokens: Mutex::new(HashMap::new()),
            gates: Mutex::new(Gates::default()),
            on_battery: AtomicBool::new(false),
            idle: Mutex::new(idle),
            stop: AtomicBool::new(false),
            subscribers: Mutex::new(Vec::new()),
            model_active: AtomicU32::new(0),
            model_total: AtomicU64::new(0),
            embed_total: AtomicU64::new(0),
        });
        let mut threads = Vec::new();
        for (r, runner) in [(Resource::Model, model_runner), (Resource::Embedder, embed_runner)] {
            let s = shared.clone();
            threads.push(
                std::thread::Builder::new()
                    .name(format!("tsr-sched-{r:?}"))
                    .spawn(move || worker(s, r, runner))
                    .expect("spawn worker"),
            );
        }
        let s = shared.clone();
        threads.push(
            std::thread::Builder::new()
                .name("tsr-sched-monitor".into())
                .spawn(move || monitor(s))
                .expect("spawn monitor"),
        );
        Runtime { shared, threads }
    }

    pub fn submit(&self, spec: JobSpec) -> Result<JobId, SubmitError> {
        let r = spec.resource;
        let id = lock(&self.shared.core).submit(spec, Instant::now())?;
        preempt_if_needed(&self.shared, r);
        self.shared.cv.notify_all();
        Ok(id)
    }

    /// Cancel a queued or running job.
    pub fn cancel(&self, id: JobId) {
        if lock(&self.shared.core).cancel_queued(id) {
            return;
        }
        if let Some(t) = lock(&self.shared.tokens).get(&id) {
            t.cancel(CancelReason::Cancelled);
        }
    }

    pub fn subscribe(&self) -> Receiver<Completion> {
        let (tx, rx) = channel();
        lock(&self.shared.subscribers).push(tx);
        rx
    }

    pub fn set_gates(&self, g: Gates) {
        *lock(&self.shared.gates) = g;
        self.shared.cv.notify_all();
    }

    pub fn set_on_battery(&self, on_battery: bool) {
        self.shared.on_battery.store(on_battery, Ordering::Release);
    }

    pub fn set_idle_policy(&self, p: IdlePolicy) {
        *lock(&self.shared.idle) = p;
    }

    pub fn sidecar_state(&self) -> State {
        lock(&self.shared.sup).state()
    }

    pub fn sidecar_pid(&self) -> Option<u32> {
        lock(&self.shared.sup).pid()
    }

    /// Run `f` with the supervisor locked (diagnostics, user "retry").
    pub fn with_supervisor<T>(&self, f: impl FnOnce(&mut Supervisor<L>) -> T) -> T {
        f(&mut lock(&self.shared.sup))
    }

    pub fn snapshot(&self) -> Snapshot {
        let now = Instant::now();
        let (depth, oldest, queue) = {
            let c = lock(&self.shared.core);
            (c.depth(), c.oldest_ms(now), c.metrics.clone())
        };
        let (state, counters, pid) = {
            let s = lock(&self.shared.sup);
            (s.state(), s.counters, s.pid())
        };
        Snapshot {
            queue_depth: depth,
            oldest_queued_ms: oldest,
            queue,
            sidecar_state: state,
            sidecar: counters,
            sidecar_process: pid.map(process_usage).unwrap_or_default(),
            model_requests_active: self.shared.model_active.load(Ordering::Acquire),
            model_requests_total: self.shared.model_total.load(Ordering::Acquire),
            embed_requests_total: self.shared.embed_total.load(Ordering::Acquire),
        }
    }

    /// Stop workers (running jobs get `Shutdown`), then stop the sidecar.
    pub fn shutdown(mut self) {
        self.stop_all();
    }

    fn stop_all(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
        for t in lock(&self.shared.tokens).values() {
            t.cancel(CancelReason::Shutdown);
        }
        self.shared.cv.notify_all();
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
        lock(&self.shared.sup).stop(Instant::now());
    }
}

impl<L: Launch + 'static> Drop for Runtime<L> {
    fn drop(&mut self) {
        if !self.threads.is_empty() {
            self.stop_all();
        }
    }
}

fn process_usage(pid: u32) -> ProcessUsage {
    #[cfg(windows)]
    {
        let u = tsr_win::query::process_usage(pid);
        ProcessUsage {
            working_set_mib: u.map(|u| u.working_set_mib),
            cpu_ms: u.map(|u| u.cpu_ms),
        }
    }
    #[cfg(not(windows))]
    {
        let _ = pid;
        ProcessUsage::default()
    }
}

fn preempt_if_needed<L: Launch>(s: &Shared<L>, r: Resource) {
    let g = *lock(&s.gates);
    let target = lock(&s.core).preempt_target(r, Instant::now(), &g);
    if let Some(id) = target
        && let Some(t) = lock(&s.tokens).get(&id)
        && t.cancel(CancelReason::Preempted)
    {
        tracing::info!(count = 1, "job preempted");
    }
}

fn notify<L: Launch>(s: &Shared<L>, c: Completion) {
    lock(&s.subscribers).retain(|tx| tx.send(c.clone()).is_ok());
}

fn worker<L: Launch>(s: Arc<Shared<L>>, r: Resource, mut runner: Box<dyn Runner>) {
    loop {
        // Wait for a job.
        let d = {
            let mut core = lock(&s.core);
            loop {
                if s.stop.load(Ordering::Acquire) {
                    return;
                }
                let g = *lock(&s.gates);
                let now = Instant::now();
                if let Some(d) = core.next(r, now, &g) {
                    break d;
                }
                let wait = core
                    .next_ready_at(r)
                    .map(|t| t.saturating_duration_since(now))
                    .unwrap_or(Duration::from_millis(500))
                    .clamp(Duration::from_millis(5), Duration::from_millis(500));
                core = s.cv.wait_timeout(core, wait).unwrap_or_else(|p| p.into_inner()).0;
            }
        };
        let token = CancelToken::new();
        lock(&s.tokens).insert(d.id, token.clone());

        let outcome = if r == Resource::Model {
            let ready = lock(&s.sup).ensure_ready(Instant::now(), &token);
            match ready {
                Ok(client) => {
                    s.model_active.fetch_add(1, Ordering::AcqRel);
                    s.model_total.fetch_add(1, Ordering::AcqRel);
                    let o = runner.run(&d, Some(&client), &token);
                    s.model_active.fetch_sub(1, Ordering::AcqRel);
                    o
                }
                Err(Unavailable::Backoff { retry_in_ms }) => Outcome::Deferred { retry_in_ms },
                Err(Unavailable::Failed) => {
                    if d.spec.priority == crate::sched::Priority::P0 {
                        // The user is waiting: tell them now instead of queueing silently.
                        Outcome::Failed { retryable: false }
                    } else {
                        Outcome::Deferred { retry_in_ms: 60_000 }
                    }
                }
                Err(Unavailable::Cancelled) => match token.reason() {
                    Some(CancelReason::Preempted) => Outcome::Preempted,
                    _ => Outcome::Cancelled,
                },
            }
        } else {
            s.embed_total.fetch_add(1, Ordering::AcqRel);
            runner.run(&d, None, &token)
        };
        // Shutdown is not the job's fault: leave it for the durable layer to re-queue on restart.
        let outcome = match (outcome, token.reason()) {
            (Outcome::Cancelled | Outcome::Preempted, Some(CancelReason::Shutdown)) => Outcome::Cancelled,
            (o, _) => o,
        };
        lock(&s.tokens).remove(&d.id);
        // Subscribers hear about final outcomes and preemptions, not deferrals or retries.
        let fin = lock(&s.core).finish(d.id, outcome, Instant::now());
        if fin == Finish::Final || (fin == Finish::Requeued && outcome == Outcome::Preempted) {
            notify(
                &s,
                Completion {
                    id: d.id,
                    key: d.spec.key.clone(),
                    outcome,
                },
            );
        }
        s.cv.notify_all();
    }
}

fn monitor<L: Launch>(s: Arc<Shared<L>>) {
    while !s.stop.load(Ordering::Acquire) {
        {
            let core = lock(&s.core);
            let _ = s.cv.wait_timeout(core, Duration::from_millis(200));
        }
        if s.stop.load(Ordering::Acquire) {
            break;
        }
        let now = Instant::now();
        for r in [Resource::Model, Resource::Embedder] {
            preempt_if_needed(&s, r);
        }
        // Health checks: skip if a worker is busy starting the sidecar (lock held).
        if let Ok(mut sup) = s.sup.try_lock() {
            sup.tick(now);
        }
        // Idle unload.
        let timeout = lock(&s.idle).timeout(s.on_battery.load(Ordering::Acquire));
        let Some(timeout) = timeout else { continue };
        let should = {
            let mut core = lock(&s.core);
            let idle_for = core
                .last_activity(Resource::Model)
                .map(|t| now.saturating_duration_since(t))
                .unwrap_or(Duration::MAX);
            // try_lock: never wait for a sidecar start while holding the queue lock.
            let go = core.idle(Resource::Model)
                && idle_for >= timeout
                && s.sup.try_lock().map(|g| g.is_loaded()).unwrap_or(false);
            if go {
                core.set_unloading(true);
            }
            go
        };
        if should {
            lock(&s.sup).unload(Instant::now());
            lock(&s.core).set_unloading(false);
            s.cv.notify_all();
        }
    }
}
