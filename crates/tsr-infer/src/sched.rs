//! Priority scheduler with preemption (plan §7.4) — the pure state machine.
//!
//! [`SchedulerCore`] decides *what runs next*; it owns no threads and never sleeps, and takes the
//! time as an argument, so the same code runs in the threaded runtime ([`crate::runtime`]) and in
//! the discrete-event backlog simulation (evals). Rules:
//!
//! - Priorities: P0 interactive, P1 near-real-time (extraction, VLM), P2 background (rollups).
//! - One job in flight per resource (the model runs with `-np 1`; the embedder has its own slot).
//! - Stable ids; a job *key* (e.g. `extract:item:42`) can be queued or running only once, and a
//!   key that completed is refused until [`SchedulerCore::forget_done`] — retries and duplicate
//!   submissions can never complete a job twice.
//! - Preemption: a queued P0 makes the running P1/P2 job on the same resource a preemption target;
//!   the runtime cancels its stream and the job is re-queued **in its old place** with its attempt
//!   count unchanged. Each job can be preempted at most `max_preemptions` times, after which it
//!   runs to completion (no livelock). Partial output of a preempted job is discarded by the
//!   runner; nothing is committed until a job finishes.
//! - Starvation: a P2 job waiting longer than `p2_promote_after` competes as P1; when P0 jobs have
//!   been dispatched `fairness_every` times in a row while a P1 job has waited longer than
//!   `p1_fairness_after`, that P1 job goes first once.
//! - Gates: when the model is not allowed (fullscreen game, presentation) only P0 jobs may use it;
//!   P2 runs only when background work is allowed (AC power unless the user opted in).
//! - Failures retry with exponential delay up to `max_attempts`; a sidecar that is temporarily
//!   unavailable *defers* the job without using an attempt.

use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::metrics::{Histogram, QueueMetrics};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum Priority {
    P0,
    P1,
    P2,
}

impl Priority {
    pub const ALL: [Priority; 3] = [Priority::P0, Priority::P1, Priority::P2];
    pub fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub enum Resource {
    /// The llama-server sidecar (GPU or CPU, whatever backend was selected).
    Model,
    /// The ONNX embedder thread (T5).
    Embedder,
}

/// A unit of work. Content-free: `kind` is a fixed label, `payload` an id (item, summary, ...).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct JobSpec {
    pub key: String,
    pub kind: &'static str,
    pub priority: Priority,
    pub resource: Resource,
    pub payload: i64,
}

pub type JobId = u64;

/// What [`SchedulerCore::finish`] did with a job.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Finish {
    /// Not running (already finished): nothing changed.
    Ignored,
    /// Back in the queue (preempted, deferred, or a retry).
    Requeued,
    /// Done, cancelled, or failed for good.
    Final,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Dispatch {
    pub id: JobId,
    pub spec: JobSpec,
    /// Failed attempts so far (0 on the first run).
    pub attempt: u32,
    pub preemptions: u32,
    pub waited_ms: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Outcome {
    Done,
    /// Stopped at a safe boundary for a higher-priority job; re-queued.
    Preempted,
    Failed {
        retryable: bool,
    },
    Cancelled,
    /// The resource is temporarily unavailable (sidecar restarting); try again later.
    Deferred {
        retry_in_ms: u64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error, Serialize)]
pub enum SubmitError {
    #[error("job already queued or running")]
    Duplicate(JobId),
    #[error("job already completed")]
    AlreadyDone,
    #[error("queue full")]
    QueueFull,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Gates {
    /// False while a fullscreen app / presentation runs or the user paused model work.
    pub model_allowed: bool,
    /// P2 work allowed (on AC power, or the user opted in to background work on battery).
    pub background_allowed: bool,
}

impl Default for Gates {
    fn default() -> Self {
        Gates {
            model_allowed: true,
            background_allowed: true,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Config {
    /// Queued jobs (all priorities). P0 may exceed it by `p0_headroom`.
    pub capacity: usize,
    pub p0_headroom: usize,
    pub max_attempts: u32,
    pub max_preemptions: u32,
    pub retry_delay: Duration,
    pub p2_promote_after: Duration,
    pub p1_fairness_after: Duration,
    pub fairness_every: u32,
    pub done_keys_cap: usize,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            capacity: 10_000,
            p0_headroom: 32,
            max_attempts: 3,
            max_preemptions: 3,
            retry_delay: Duration::from_secs(5),
            p2_promote_after: Duration::from_secs(6 * 3600),
            p1_fairness_after: Duration::from_secs(5 * 60),
            fairness_every: 4,
            done_keys_cap: 50_000,
        }
    }
}

#[derive(Clone, Debug)]
struct Entry {
    id: JobId,
    spec: JobSpec,
    enqueued_at: Instant,
    ready_at: Instant,
    attempts: u32,
    preemptions: u32,
}

#[derive(Clone, Debug)]
struct Running {
    entry: Entry,
    started_at: Instant,
}

pub struct SchedulerCore {
    cfg: Config,
    next_id: JobId,
    queued: Vec<Entry>,
    running: HashMap<Resource, Running>,
    by_key: HashMap<String, JobId>,
    done: HashSet<String>,
    done_order: VecDeque<String>,
    consecutive_p0: u32,
    /// Model dispatch is blocked while the sidecar is being unloaded.
    unloading: bool,
    last_activity: HashMap<Resource, Instant>,
    pub metrics: QueueMetrics,
}

impl SchedulerCore {
    pub fn new(cfg: Config) -> Self {
        SchedulerCore {
            cfg,
            next_id: 1,
            queued: Vec::new(),
            running: HashMap::new(),
            by_key: HashMap::new(),
            done: HashSet::new(),
            done_order: VecDeque::new(),
            consecutive_p0: 0,
            unloading: false,
            last_activity: HashMap::new(),
            metrics: QueueMetrics::default(),
        }
    }

    pub fn config(&self) -> &Config {
        &self.cfg
    }

    pub fn submit(&mut self, spec: JobSpec, now: Instant) -> Result<JobId, SubmitError> {
        if let Some(&id) = self.by_key.get(&spec.key) {
            self.metrics.duplicates += 1;
            return Err(SubmitError::Duplicate(id));
        }
        if self.done.contains(&spec.key) {
            self.metrics.duplicates += 1;
            return Err(SubmitError::AlreadyDone);
        }
        let limit = self.cfg.capacity
            + if spec.priority == Priority::P0 {
                self.cfg.p0_headroom
            } else {
                0
            };
        if self.queued.len() >= limit {
            self.metrics.rejected_full += 1;
            return Err(SubmitError::QueueFull);
        }
        let id = self.next_id;
        self.next_id += 1;
        self.by_key.insert(spec.key.clone(), id);
        self.metrics.submitted[spec.priority.index()] += 1;
        self.queued.push(Entry {
            id,
            spec,
            enqueued_at: now,
            ready_at: now,
            attempts: 0,
            preemptions: 0,
        });
        Ok(id)
    }

    /// Allow a completed key to be submitted again (e.g. re-extraction with a new prompt version).
    pub fn forget_done(&mut self, key: &str) {
        if self.done.remove(key) {
            self.done_order.retain(|k| k != key);
        }
    }

    fn effective(&self, e: &Entry, now: Instant) -> Priority {
        if e.spec.priority == Priority::P2 && now.duration_since(e.enqueued_at) >= self.cfg.p2_promote_after {
            Priority::P1
        } else {
            e.spec.priority
        }
    }

    fn eligible(&self, e: &Entry, r: Resource, now: Instant, g: &Gates) -> bool {
        if e.spec.resource != r || e.ready_at > now {
            return false;
        }
        let p = self.effective(e, now);
        if r == Resource::Model && !g.model_allowed && p != Priority::P0 {
            return false;
        }
        if r == Resource::Model && self.unloading {
            return false;
        }
        p != Priority::P2 || g.background_allowed
    }

    /// Pick the next job for `resource`, or `None` if it is busy or nothing is eligible.
    pub fn next(&mut self, r: Resource, now: Instant, g: &Gates) -> Option<Dispatch> {
        if self.running.contains_key(&r) {
            return None;
        }
        let best = self
            .queued
            .iter()
            .enumerate()
            .filter(|(_, e)| self.eligible(e, r, now, g))
            .min_by_key(|(_, e)| (self.effective(e, now), e.enqueued_at, e.id))
            .map(|(i, e)| (i, self.effective(e, now)))?;
        let mut pick = best.0;
        if best.1 == Priority::P0 && self.consecutive_p0 >= self.cfg.fairness_every {
            // Let one long-waiting P1 job through.
            if let Some((i, _)) = self
                .queued
                .iter()
                .enumerate()
                .filter(|(_, e)| self.eligible(e, r, now, g))
                .filter(|(_, e)| {
                    self.effective(e, now) == Priority::P1
                        && now.duration_since(e.enqueued_at) >= self.cfg.p1_fairness_after
                })
                .min_by_key(|(_, e)| (e.enqueued_at, e.id))
            {
                pick = i;
                self.metrics.fairness_grants += 1;
            }
        }
        let e = self.queued.swap_remove(pick);
        if self.effective(&e, now) == Priority::P0 {
            self.consecutive_p0 += 1;
        } else {
            self.consecutive_p0 = 0;
        }
        let waited = now.duration_since(e.enqueued_at);
        self.metrics.wait_ms[e.spec.priority.index()].record(waited.as_millis() as u64);
        self.metrics.dispatched += 1;
        self.last_activity.insert(r, now);
        let d = Dispatch {
            id: e.id,
            spec: e.spec.clone(),
            attempt: e.attempts,
            preemptions: e.preemptions,
            waited_ms: waited.as_millis() as u64,
        };
        self.running.insert(
            r,
            Running {
                entry: e,
                started_at: now,
            },
        );
        Some(d)
    }

    /// The running job on `r` that should be preempted now, if any.
    pub fn preempt_target(&self, r: Resource, now: Instant, g: &Gates) -> Option<JobId> {
        let run = self.running.get(&r)?;
        if run.entry.spec.priority == Priority::P0 || run.entry.preemptions >= self.cfg.max_preemptions {
            return None;
        }
        let waiting_p0 = self
            .queued
            .iter()
            .any(|e| e.spec.priority == Priority::P0 && self.eligible(e, r, now, g));
        waiting_p0.then_some(run.entry.id)
    }

    pub fn running(&self, r: Resource) -> Option<JobId> {
        self.running.get(&r).map(|x| x.entry.id)
    }

    /// Record how a dispatched job ended. Unknown ids (already finished) are ignored, so a late
    /// duplicate completion cannot double-count or re-queue.
    pub fn finish(&mut self, id: JobId, outcome: Outcome, now: Instant) -> Finish {
        let Some(r) = self.running.iter().find(|(_, x)| x.entry.id == id).map(|(r, _)| *r) else {
            return Finish::Ignored;
        };
        let Running { mut entry, started_at } = self.running.remove(&r).expect("found");
        self.last_activity.insert(r, now);
        let ran = now.duration_since(started_at).as_millis() as u64;
        match outcome {
            Outcome::Done => {
                self.by_key.remove(&entry.spec.key);
                self.remember_done(entry.spec.key.clone());
                self.metrics.completed[entry.spec.priority.index()] += 1;
                self.metrics.run_ms.entry(entry.spec.kind).or_default().record(ran);
                self.metrics
                    .latency_ms
                    .entry(entry.spec.kind)
                    .or_default()
                    .record(now.duration_since(entry.enqueued_at).as_millis() as u64);
            }
            Outcome::Preempted => {
                entry.preemptions += 1;
                entry.ready_at = now;
                self.metrics.preemptions += 1;
                self.queued.push(entry);
                return Finish::Requeued;
            }
            Outcome::Deferred { retry_in_ms } => {
                entry.ready_at = now + Duration::from_millis(retry_in_ms.max(10));
                self.metrics.deferrals += 1;
                self.queued.push(entry);
                return Finish::Requeued;
            }
            Outcome::Failed { retryable } => {
                entry.attempts += 1;
                if retryable && entry.attempts < self.cfg.max_attempts {
                    let d = self.cfg.retry_delay * 2u32.pow(entry.attempts.saturating_sub(1).min(10));
                    entry.ready_at = now + d;
                    self.metrics.retries += 1;
                    self.queued.push(entry);
                    return Finish::Requeued;
                } else {
                    self.by_key.remove(&entry.spec.key);
                    self.metrics.failed += 1;
                }
            }
            Outcome::Cancelled => {
                self.by_key.remove(&entry.spec.key);
                self.metrics.cancelled += 1;
            }
        }
        Finish::Final
    }

    /// Remove a queued (not running) job. Returns whether it was found.
    pub fn cancel_queued(&mut self, id: JobId) -> bool {
        if let Some(i) = self.queued.iter().position(|e| e.id == id) {
            let e = self.queued.swap_remove(i);
            self.by_key.remove(&e.spec.key);
            self.metrics.cancelled += 1;
            true
        } else {
            false
        }
    }

    fn remember_done(&mut self, key: String) {
        if self.done.insert(key.clone()) {
            self.done_order.push_back(key);
            while self.done_order.len() > self.cfg.done_keys_cap {
                if let Some(old) = self.done_order.pop_front() {
                    self.done.remove(&old);
                }
            }
        }
    }

    /// Nothing queued or running for `r`.
    pub fn idle(&self, r: Resource) -> bool {
        !self.running.contains_key(&r) && !self.queued.iter().any(|e| e.spec.resource == r)
    }

    pub fn last_activity(&self, r: Resource) -> Option<Instant> {
        self.last_activity.get(&r).copied()
    }

    pub fn set_unloading(&mut self, on: bool) {
        self.unloading = on;
    }

    pub fn is_unloading(&self) -> bool {
        self.unloading
    }

    /// Earliest instant a currently ineligible-for-time job becomes ready (for timed waits).
    pub fn next_ready_at(&self, r: Resource) -> Option<Instant> {
        self.queued
            .iter()
            .filter(|e| e.spec.resource == r)
            .map(|e| e.ready_at)
            .min()
    }

    pub fn depth(&self) -> [usize; 3] {
        let mut d = [0; 3];
        for e in &self.queued {
            d[e.spec.priority.index()] += 1;
        }
        d
    }

    /// Age of the oldest queued job per priority, in ms.
    pub fn oldest_ms(&self, now: Instant) -> [u64; 3] {
        let mut o = [0u64; 3];
        for e in &self.queued {
            let a = now.duration_since(e.enqueued_at).as_millis() as u64;
            let i = e.spec.priority.index();
            o[i] = o[i].max(a);
        }
        o
    }

    pub fn queued_len(&self) -> usize {
        self.queued.len()
    }

    pub fn run_histogram(&self, kind: &str) -> Option<&Histogram> {
        self.metrics.run_ms.get(kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(key: &str, p: Priority) -> JobSpec {
        JobSpec {
            key: key.into(),
            kind: "test",
            priority: p,
            resource: Resource::Model,
            payload: 0,
        }
    }

    fn core() -> SchedulerCore {
        SchedulerCore::new(Config::default())
    }

    const G: Gates = Gates {
        model_allowed: true,
        background_allowed: true,
    };

    #[test]
    fn priority_then_fifo_order() {
        let mut c = core();
        let t = Instant::now();
        c.submit(job("a", Priority::P2), t).unwrap();
        c.submit(job("b", Priority::P1), t + Duration::from_millis(1)).unwrap();
        c.submit(job("c", Priority::P1), t + Duration::from_millis(2)).unwrap();
        c.submit(job("d", Priority::P0), t + Duration::from_millis(3)).unwrap();
        let mut order = vec![];
        let now = t + Duration::from_secs(1);
        while let Some(d) = c.next(Resource::Model, now, &G) {
            order.push(d.spec.key.clone());
            c.finish(d.id, Outcome::Done, now);
        }
        assert_eq!(order, ["d", "b", "c", "a"]);
    }

    #[test]
    fn one_job_in_flight_per_resource() {
        let mut c = core();
        let t = Instant::now();
        c.submit(job("a", Priority::P1), t).unwrap();
        c.submit(job("b", Priority::P1), t).unwrap();
        let mut e = job("e", Priority::P1);
        e.resource = Resource::Embedder;
        c.submit(e, t).unwrap();
        assert!(c.next(Resource::Model, t, &G).is_some());
        assert!(c.next(Resource::Model, t, &G).is_none());
        assert!(c.next(Resource::Embedder, t, &G).is_some(), "embedder has its own slot");
    }

    #[test]
    fn duplicates_and_completed_keys_are_refused() {
        let mut c = core();
        let t = Instant::now();
        let id = c.submit(job("x", Priority::P1), t).unwrap();
        assert_eq!(c.submit(job("x", Priority::P1), t), Err(SubmitError::Duplicate(id)));
        let d = c.next(Resource::Model, t, &G).unwrap();
        assert_eq!(c.submit(job("x", Priority::P0), t), Err(SubmitError::Duplicate(id)));
        assert_eq!(c.finish(d.id, Outcome::Done, t), Finish::Final);
        assert_eq!(
            c.finish(d.id, Outcome::Done, t),
            Finish::Ignored,
            "second completion ignored"
        );
        assert_eq!(c.metrics.completed[1], 1);
        assert_eq!(c.submit(job("x", Priority::P1), t), Err(SubmitError::AlreadyDone));
        c.forget_done("x");
        assert!(c.submit(job("x", Priority::P1), t).is_ok());
    }

    #[test]
    fn p0_preempts_p1_which_keeps_its_place() {
        let mut c = core();
        let t = Instant::now();
        c.submit(job("p1-old", Priority::P1), t).unwrap();
        c.submit(job("p1-new", Priority::P1), t + Duration::from_millis(5))
            .unwrap();
        let d = c.next(Resource::Model, t, &G).unwrap();
        assert_eq!(d.spec.key, "p1-old");
        assert_eq!(c.preempt_target(Resource::Model, t, &G), None);
        c.submit(job("ask", Priority::P0), t + Duration::from_millis(10))
            .unwrap();
        assert_eq!(
            c.preempt_target(Resource::Model, t + Duration::from_millis(10), &G),
            Some(d.id)
        );
        c.finish(d.id, Outcome::Preempted, t + Duration::from_millis(11));
        let a = c.next(Resource::Model, t + Duration::from_millis(12), &G).unwrap();
        assert_eq!(a.spec.key, "ask");
        c.finish(a.id, Outcome::Done, t + Duration::from_millis(13));
        let again = c.next(Resource::Model, t + Duration::from_millis(14), &G).unwrap();
        assert_eq!(
            (again.spec.key.as_str(), again.attempt, again.preemptions),
            ("p1-old", 0, 1)
        );
        assert_eq!(c.metrics.preemptions, 1);
    }

    #[test]
    fn preemption_is_bounded_per_job() {
        let mut c = core();
        let t = Instant::now();
        c.submit(job("long", Priority::P2), t).unwrap();
        for i in 0..5 {
            let d = c.next(Resource::Model, t, &G).unwrap();
            assert_eq!(d.spec.key, "long");
            c.submit(job(&format!("ask{i}"), Priority::P0), t).unwrap();
            let target = c.preempt_target(Resource::Model, t, &G);
            if i < 3 {
                assert_eq!(target, Some(d.id));
                c.finish(d.id, Outcome::Preempted, t);
                let a = c.next(Resource::Model, t, &G).unwrap();
                c.finish(a.id, Outcome::Done, t);
            } else {
                assert_eq!(target, None, "after 3 preemptions the job runs to completion");
                c.finish(d.id, Outcome::Done, t);
                break;
            }
        }
    }

    #[test]
    fn p0_never_preempts_p0() {
        let mut c = core();
        let t = Instant::now();
        c.submit(job("a", Priority::P0), t).unwrap();
        c.next(Resource::Model, t, &G).unwrap();
        c.submit(job("b", Priority::P0), t).unwrap();
        assert_eq!(c.preempt_target(Resource::Model, t, &G), None);
    }

    #[test]
    fn retries_back_off_then_give_up() {
        let mut c = core();
        let t = Instant::now();
        c.submit(job("f", Priority::P1), t).unwrap();
        let d = c.next(Resource::Model, t, &G).unwrap();
        c.finish(d.id, Outcome::Failed { retryable: true }, t);
        assert!(
            c.next(Resource::Model, t + Duration::from_secs(4), &G).is_none(),
            "5 s retry delay"
        );
        let d = c.next(Resource::Model, t + Duration::from_secs(5), &G).unwrap();
        assert_eq!(d.attempt, 1);
        c.finish(d.id, Outcome::Failed { retryable: true }, t + Duration::from_secs(5));
        assert!(
            c.next(Resource::Model, t + Duration::from_secs(14), &G).is_none(),
            "10 s"
        );
        let d = c.next(Resource::Model, t + Duration::from_secs(15), &G).unwrap();
        c.finish(d.id, Outcome::Failed { retryable: true }, t + Duration::from_secs(15));
        assert!(c.idle(Resource::Model), "gave up after 3 attempts");
        assert_eq!(c.metrics.failed, 1);
        // A failed key may be submitted again later (e.g. after restart).
        assert!(c.submit(job("f", Priority::P1), t).is_ok());
    }

    #[test]
    fn deferral_uses_no_attempt() {
        let mut c = core();
        let t = Instant::now();
        c.submit(job("d", Priority::P1), t).unwrap();
        for _ in 0..10 {
            let x = c.next(Resource::Model, t + Duration::from_secs(100), &G).unwrap();
            assert_eq!(x.attempt, 0);
            c.finish(x.id, Outcome::Deferred { retry_in_ms: 1 }, t);
        }
    }

    #[test]
    fn gates_hold_background_and_non_interactive_model_work() {
        let mut c = core();
        let t = Instant::now();
        c.submit(job("p2", Priority::P2), t).unwrap();
        c.submit(job("p1", Priority::P1), t).unwrap();
        let fullscreen = Gates {
            model_allowed: false,
            background_allowed: true,
        };
        assert!(c.next(Resource::Model, t, &fullscreen).is_none());
        c.submit(job("ask", Priority::P0), t).unwrap();
        assert_eq!(
            c.next(Resource::Model, t, &fullscreen).unwrap().spec.key,
            "ask",
            "user asked explicitly"
        );
        let id = c.running(Resource::Model).unwrap();
        c.finish(id, Outcome::Done, t);
        let battery = Gates {
            model_allowed: true,
            background_allowed: false,
        };
        assert_eq!(c.next(Resource::Model, t, &battery).unwrap().spec.key, "p1");
        let id = c.running(Resource::Model).unwrap();
        c.finish(id, Outcome::Done, t);
        assert!(c.next(Resource::Model, t, &battery).is_none(), "P2 waits for AC");
        // ... unless it has waited long enough to be promoted.
        assert!(
            c.next(Resource::Model, t + Duration::from_secs(6 * 3600), &battery)
                .is_some()
        );
    }

    #[test]
    fn unloading_blocks_model_dispatch() {
        let mut c = core();
        let t = Instant::now();
        c.submit(job("ask", Priority::P0), t).unwrap();
        c.set_unloading(true);
        assert!(c.next(Resource::Model, t, &G).is_none());
        c.set_unloading(false);
        assert!(c.next(Resource::Model, t, &G).is_some());
    }

    #[test]
    fn long_waiting_p1_gets_through_a_stream_of_p0() {
        let mut c = core();
        let t = Instant::now();
        c.submit(job("p1", Priority::P1), t).unwrap();
        let later = t + Duration::from_secs(301);
        let mut order = vec![];
        for i in 0..6 {
            c.submit(job(&format!("ask{i}"), Priority::P0), later).unwrap();
        }
        while let Some(d) = c.next(Resource::Model, later, &G) {
            order.push(d.spec.key.clone());
            c.finish(d.id, Outcome::Done, later);
        }
        assert_eq!(order.iter().position(|k| k == "p1"), Some(4), "{order:?}");
        assert_eq!(c.metrics.fairness_grants, 1);
    }

    #[test]
    fn queue_is_bounded_with_p0_headroom() {
        let mut c = SchedulerCore::new(Config {
            capacity: 2,
            p0_headroom: 1,
            ..Config::default()
        });
        let t = Instant::now();
        c.submit(job("1", Priority::P1), t).unwrap();
        c.submit(job("2", Priority::P1), t).unwrap();
        assert_eq!(c.submit(job("3", Priority::P1), t), Err(SubmitError::QueueFull));
        assert!(c.submit(job("ask", Priority::P0), t).is_ok());
        assert_eq!(c.submit(job("ask2", Priority::P0), t), Err(SubmitError::QueueFull));
    }

    #[test]
    fn idle_and_metrics_are_content_free_counts() {
        let mut c = core();
        let t = Instant::now();
        assert!(c.idle(Resource::Model));
        c.submit(job("a", Priority::P1), t).unwrap();
        assert!(!c.idle(Resource::Model));
        assert_eq!(c.depth(), [0, 1, 0]);
        assert_eq!(c.oldest_ms(t + Duration::from_secs(2)), [0, 2000, 0]);
        let d = c.next(Resource::Model, t + Duration::from_secs(2), &G).unwrap();
        c.finish(d.id, Outcome::Done, t + Duration::from_secs(3));
        assert!(c.idle(Resource::Model));
        let json = serde_json::to_string(&c.metrics).unwrap();
        assert!(!json.contains("\"a\""), "job keys are not in metrics");
    }
}
