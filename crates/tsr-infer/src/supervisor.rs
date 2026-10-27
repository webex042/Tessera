//! Sidecar lifecycle: an explicit state machine with health checks, bounded restarts with
//! exponential backoff and jitter, and idle unload (plan §4.3, §7.1).
//!
//! ```text
//!            ensure_ready                 health ok
//!  Stopped ───────────────► Starting ──────────────► Ready ◄──────┐
//!     ▲                        │ fail                 │ miss ×N    │ ok
//!     │ unload / stop          ▼                      ▼ or exit    │
//!     └──────────────────── Backoff ◄─────────── (kill) ──────────┘
//!                              │ budget spent (5 restarts / hour)
//!                              ▼
//!                           Failed ── after the window, or reset() ──► Stopped
//! ```
//!
//! Only one process exists at a time: every transition happens under `&mut self`, and the runtime
//! keeps the supervisor behind one mutex. Time is passed in (`now`) so tests are deterministic;
//! the supervisor never sleeps — callers wait until the instant it reports.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::cancel::CancelToken;
use crate::client::LoopbackClient;
use crate::sidecar::LaunchError;

/// A running sidecar as the supervisor sees it (real process or a test double).
pub trait Proc: Send {
    fn client(&self) -> &LoopbackClient;
    /// `Some(code)` once it has exited.
    fn exited(&mut self) -> Option<Option<i32>>;
    /// Health probe with a short timeout. `Ok(true)` = ready.
    fn healthy(&mut self, timeout: Duration) -> bool;
    fn kill(&mut self);
    fn pid(&self) -> u32;
}

pub trait Launch: Send {
    type P: Proc;
    fn launch(&mut self, cancel: &CancelToken) -> Result<Self::P, LaunchError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum State {
    Stopped,
    Starting,
    Ready,
    Backoff,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum Event {
    Starting { attempt: u32 },
    Ready { pid: u32, ms: u64 },
    LaunchFailed { error: LaunchError },
    HealthMiss { misses: u32 },
    Exited { code: Option<i32> },
    Hung,
    BackoffScheduled { ms: u64 },
    BudgetExhausted,
    Unloaded,
    Stopped,
    Reset,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Unavailable {
    /// Restarting after a failure; try again at this instant.
    Backoff {
        retry_in_ms: u64,
    },
    /// Restart budget spent; "model unavailable" until the window passes or the user retries.
    Failed,
    Cancelled,
}

#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub health_interval: Duration,
    pub health_timeout: Duration,
    /// Consecutive failed health checks before the process is considered hung.
    pub misses_before_restart: u32,
    pub backoff_initial: Duration,
    pub backoff_max: Duration,
    /// ± fraction applied to each backoff delay.
    pub jitter: f64,
    pub max_restarts: usize,
    pub restart_window: Duration,
    /// Healthy this long → backoff starts from `backoff_initial` again.
    pub stable_after: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            health_interval: Duration::from_secs(5),
            health_timeout: Duration::from_secs(2),
            misses_before_restart: 3,
            backoff_initial: Duration::from_secs(2),
            backoff_max: Duration::from_secs(60),
            jitter: 0.2,
            max_restarts: 5,
            restart_window: Duration::from_secs(3600),
            stable_after: Duration::from_secs(600),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct Counters {
    pub starts: u64,
    pub launch_failures: u64,
    pub crashes: u64,
    pub hangs: u64,
    pub restarts: u64,
    pub unloads: u64,
    pub last_ready_ms: u64,
    pub last_unload_ms: u64,
}

pub struct Supervisor<L: Launch> {
    cfg: Config,
    launcher: L,
    state: State,
    proc: Option<L::P>,
    /// Failure-triggered restarts inside the window.
    restarts: VecDeque<Instant>,
    attempt: u32,
    retry_at: Option<Instant>,
    ready_since: Option<Instant>,
    last_health: Option<Instant>,
    misses: u32,
    rng: u64,
    events: VecDeque<(Instant, Event)>,
    pub counters: Counters,
}

const MAX_EVENTS: usize = 256;

impl<L: Launch> Supervisor<L> {
    pub fn new(cfg: Config, launcher: L, seed: u64) -> Self {
        Supervisor {
            cfg,
            launcher,
            state: State::Stopped,
            proc: None,
            restarts: VecDeque::new(),
            attempt: 0,
            retry_at: None,
            ready_since: None,
            last_health: None,
            misses: 0,
            rng: seed | 1,
            events: VecDeque::new(),
            counters: Counters::default(),
        }
    }

    pub fn state(&self) -> State {
        self.state
    }

    pub fn events(&self) -> impl Iterator<Item = &(Instant, Event)> {
        self.events.iter()
    }

    pub fn pid(&self) -> Option<u32> {
        self.proc.as_ref().map(|p| p.pid())
    }

    pub fn launcher(&self) -> &L {
        &self.launcher
    }

    fn emit(&mut self, now: Instant, e: Event) {
        match &e {
            Event::Ready { ms, .. } => {
                tracing::info!(ms = *ms, "sidecar ready");
            }
            Event::LaunchFailed { error } => {
                tracing::warn!(error = %error, "sidecar launch failed");
            }
            Event::Exited { code } => {
                tracing::warn!(error_code = code.unwrap_or(-1), "sidecar exited");
            }
            Event::Hung => {
                tracing::warn!(count = self.misses, "sidecar hung");
            }
            Event::BackoffScheduled { ms } => {
                tracing::info!(ms = *ms, "sidecar restart scheduled");
            }
            Event::BudgetExhausted => {
                tracing::warn!(count = self.restarts.len(), "sidecar restart budget exhausted");
            }
            Event::Unloaded => {
                tracing::info!("sidecar unloaded");
            }
            _ => {}
        }
        if self.events.len() == MAX_EVENTS {
            self.events.pop_front();
        }
        self.events.push_back((now, e));
    }

    fn next_rand(&mut self) -> f64 {
        // xorshift64*: deterministic per seed, good enough for jitter.
        self.rng ^= self.rng >> 12;
        self.rng ^= self.rng << 25;
        self.rng ^= self.rng >> 27;
        (self.rng.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Delay before restart number `attempt` (0-based).
    pub fn backoff_delay(&mut self, attempt: u32) -> Duration {
        let base = self.cfg.backoff_initial.as_secs_f64() * 2f64.powi(attempt.min(30) as i32);
        let base = base.min(self.cfg.backoff_max.as_secs_f64());
        let j = 1.0 + self.cfg.jitter * (2.0 * self.next_rand() - 1.0);
        Duration::from_secs_f64((base * j).clamp(0.0, self.cfg.backoff_max.as_secs_f64() * (1.0 + self.cfg.jitter)))
    }

    fn prune(&mut self, now: Instant) {
        while let Some(&t) = self.restarts.front() {
            if now.duration_since(t) >= self.cfg.restart_window {
                self.restarts.pop_front();
            } else {
                break;
            }
        }
    }

    /// After a failure: either schedule a restart or give up for the window.
    fn after_failure(&mut self, now: Instant) {
        self.proc = None;
        self.ready_since = None;
        self.prune(now);
        if self.restarts.len() >= self.cfg.max_restarts {
            self.state = State::Failed;
            self.retry_at = None;
            self.emit(now, Event::BudgetExhausted);
            return;
        }
        self.restarts.push_back(now);
        self.counters.restarts += 1;
        let d = self.backoff_delay(self.attempt);
        self.attempt += 1;
        self.retry_at = Some(now + d);
        self.state = State::Backoff;
        self.emit(
            now,
            Event::BackoffScheduled {
                ms: d.as_millis() as u64,
            },
        );
    }

    fn start(&mut self, now: Instant, cancel: &CancelToken) -> Result<(), Unavailable> {
        self.state = State::Starting;
        self.counters.starts += 1;
        self.emit(now, Event::Starting { attempt: self.attempt });
        let t = Instant::now();
        match self.launcher.launch(cancel) {
            Ok(p) => {
                let ms = t.elapsed().as_millis() as u64;
                self.counters.last_ready_ms = ms;
                let pid = p.pid();
                self.proc = Some(p);
                self.state = State::Ready;
                self.ready_since = Some(now);
                self.last_health = Some(now);
                self.misses = 0;
                self.emit(now, Event::Ready { pid, ms });
                Ok(())
            }
            Err(LaunchError::Cancelled) => {
                self.state = State::Stopped;
                Err(Unavailable::Cancelled)
            }
            Err(error) => {
                self.counters.launch_failures += 1;
                self.emit(now, Event::LaunchFailed { error });
                self.after_failure(now);
                Err(self.unavailable(now))
            }
        }
    }

    fn unavailable(&self, now: Instant) -> Unavailable {
        match (self.state, self.retry_at) {
            (State::Backoff, Some(t)) => Unavailable::Backoff {
                retry_in_ms: t.saturating_duration_since(now).as_millis() as u64,
            },
            _ => Unavailable::Failed,
        }
    }

    /// Make sure a healthy sidecar is running and return its client. Never blocks on backoff.
    pub fn ensure_ready(&mut self, now: Instant, cancel: &CancelToken) -> Result<LoopbackClient, Unavailable> {
        if cancel.is_cancelled() {
            return Err(Unavailable::Cancelled);
        }
        if self.state == State::Ready
            && let Some(p) = self.proc.as_mut()
            && let Some(code) = p.exited()
        {
            self.counters.crashes += 1;
            self.emit(now, Event::Exited { code });
            self.after_failure(now);
        }
        match self.state {
            State::Ready => {}
            State::Stopped => self.start(now, cancel)?,
            State::Backoff => {
                if self.retry_at.is_some_and(|t| now < t) {
                    return Err(self.unavailable(now));
                }
                self.start(now, cancel)?;
            }
            State::Failed => {
                self.prune(now);
                if self.restarts.len() >= self.cfg.max_restarts {
                    return Err(Unavailable::Failed);
                }
                self.start(now, cancel)?;
            }
            State::Starting => return Err(Unavailable::Backoff { retry_in_ms: 100 }),
        }
        Ok(self.proc.as_ref().expect("ready implies a process").client().clone())
    }

    /// Periodic monitoring (call every few hundred ms; it rate-limits itself).
    pub fn tick(&mut self, now: Instant) {
        if self.state != State::Ready {
            return;
        }
        let Some(p) = self.proc.as_mut() else { return };
        if let Some(code) = p.exited() {
            self.counters.crashes += 1;
            self.emit(now, Event::Exited { code });
            self.after_failure(now);
            return;
        }
        if self
            .last_health
            .is_some_and(|t| now.duration_since(t) < self.cfg.health_interval)
        {
            return;
        }
        self.last_health = Some(now);
        let timeout = self.cfg.health_timeout;
        if self.proc.as_mut().expect("checked").healthy(timeout) {
            self.misses = 0;
            if self
                .ready_since
                .is_some_and(|t| now.duration_since(t) >= self.cfg.stable_after)
            {
                self.attempt = 0;
            }
            return;
        }
        self.misses += 1;
        self.emit(now, Event::HealthMiss { misses: self.misses });
        if self.misses >= self.cfg.misses_before_restart {
            self.counters.hangs += 1;
            self.emit(now, Event::Hung);
            if let Some(mut p) = self.proc.take() {
                p.kill();
            }
            self.misses = 0;
            self.after_failure(now);
        }
    }

    /// A request failed in a way that suggests the process is gone; check right away.
    pub fn report_connection_problem(&mut self, now: Instant) {
        self.last_health = None;
        self.tick(now);
    }

    /// Idle unload or user stop: kill the process. Not a failure, so no restart budget is used;
    /// the next `ensure_ready` starts it again.
    pub fn unload(&mut self, now: Instant) {
        if let Some(mut p) = self.proc.take() {
            let t = Instant::now();
            p.kill();
            self.counters.unloads += 1;
            self.counters.last_unload_ms = t.elapsed().as_millis() as u64;
            self.emit(now, Event::Unloaded);
        }
        if matches!(self.state, State::Ready | State::Starting) {
            self.state = State::Stopped;
        }
        self.ready_since = None;
    }

    pub fn stop(&mut self, now: Instant) {
        self.unload(now);
        self.state = State::Stopped;
        self.retry_at = None;
        self.emit(now, Event::Stopped);
    }

    /// User pressed "retry" after "model unavailable".
    pub fn reset(&mut self, now: Instant) {
        self.restarts.clear();
        self.attempt = 0;
        self.retry_at = None;
        if self.state != State::Ready {
            self.state = State::Stopped;
        }
        self.emit(now, Event::Reset);
    }

    pub fn is_loaded(&self) -> bool {
        self.proc.is_some()
    }
}

/// Real launcher: one [`crate::sidecar::SidecarSpec`].
pub struct SpecLauncher(pub crate::sidecar::SidecarSpec);

impl Proc for crate::sidecar::SidecarProcess {
    fn client(&self) -> &LoopbackClient {
        crate::sidecar::SidecarProcess::client(self)
    }
    fn exited(&mut self) -> Option<Option<i32>> {
        crate::sidecar::SidecarProcess::exited(self)
    }
    fn healthy(&mut self, timeout: Duration) -> bool {
        matches!(self.client().health(timeout), Ok(crate::client::Health::Ready))
    }
    fn kill(&mut self) {
        crate::sidecar::SidecarProcess::kill(self)
    }
    fn pid(&self) -> u32 {
        crate::sidecar::SidecarProcess::pid(self)
    }
}

impl Launch for SpecLauncher {
    type P = crate::sidecar::SidecarProcess;
    fn launch(&mut self, cancel: &CancelToken) -> Result<Self::P, LaunchError> {
        crate::sidecar::start(&self.0, cancel)
    }
}

/// Real launcher over backend candidates (best first, CPU last): tries each in order, and
/// starts with the one that last worked (plan §7.1 "start with fallback").
pub struct CandidatesLauncher {
    pub specs: Vec<crate::sidecar::SidecarSpec>,
    pub labels: Vec<String>,
    last_good: Option<usize>,
}

impl CandidatesLauncher {
    pub fn new(specs: Vec<crate::sidecar::SidecarSpec>, labels: Vec<String>) -> Self {
        CandidatesLauncher {
            specs,
            labels,
            last_good: None,
        }
    }

    /// Label of the candidate that started last time (diagnostics).
    pub fn active(&self) -> Option<&str> {
        self.last_good.and_then(|i| self.labels.get(i)).map(String::as_str)
    }
}

impl Launch for CandidatesLauncher {
    type P = crate::sidecar::SidecarProcess;
    fn launch(&mut self, cancel: &CancelToken) -> Result<Self::P, LaunchError> {
        let mut order: Vec<usize> = (0..self.specs.len()).collect();
        if let Some(g) = self.last_good {
            order.retain(|&i| i != g);
            order.insert(0, g);
        }
        let mut last = LaunchError::NoPort;
        for i in order {
            match crate::sidecar::start(&self.specs[i], cancel) {
                Ok(p) => {
                    self.last_good = Some(i);
                    return Ok(p);
                }
                Err(LaunchError::Cancelled) => return Err(LaunchError::Cancelled),
                Err(e) => {
                    tracing::warn!(candidate = %self.labels.get(i).map(String::as_str).unwrap_or("?"), error = %e,
                                   "backend initialisation failed; trying next");
                    last = e;
                }
            }
        }
        Err(last)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// SIMULATED process: behaviour scripted per launch.
    #[derive(Clone, Default)]
    pub struct Script {
        /// Results of successive launches (`Err` = launch fails). Missing entries succeed.
        pub launches: Arc<Mutex<VecDeque<Result<(), LaunchError>>>>,
        pub healthy: Arc<Mutex<bool>>,
        pub exited: Arc<Mutex<Option<Option<i32>>>>,
        pub live: Arc<Mutex<u32>>,
    }

    pub struct FakeProc {
        s: Script,
        c: LoopbackClient,
        pid: u32,
        killed: bool,
    }

    impl Proc for FakeProc {
        fn client(&self) -> &LoopbackClient {
            &self.c
        }
        fn exited(&mut self) -> Option<Option<i32>> {
            *self.s.exited.lock().unwrap()
        }
        fn healthy(&mut self, _: Duration) -> bool {
            *self.s.healthy.lock().unwrap()
        }
        fn kill(&mut self) {
            if !self.killed {
                self.killed = true;
                *self.s.live.lock().unwrap() -= 1;
            }
        }
        fn pid(&self) -> u32 {
            self.pid
        }
    }

    impl Drop for FakeProc {
        fn drop(&mut self) {
            self.kill();
        }
    }

    pub struct FakeLaunch(pub Script, pub u32);

    impl Launch for FakeLaunch {
        type P = FakeProc;
        fn launch(&mut self, _: &CancelToken) -> Result<FakeProc, LaunchError> {
            if let Some(Err(e)) = self.0.launches.lock().unwrap().pop_front() {
                return Err(e);
            }
            self.1 += 1;
            *self.0.healthy.lock().unwrap() = true;
            *self.0.exited.lock().unwrap() = None;
            let mut live = self.0.live.lock().unwrap();
            *live += 1;
            assert_eq!(*live, 1, "two sidecars alive at once");
            Ok(FakeProc {
                s: self.0.clone(),
                c: LoopbackClient::new("127.0.0.1:9".parse().unwrap(), "k").unwrap(),
                pid: self.1,
                killed: false,
            })
        }
    }

    fn sup(s: &Script) -> Supervisor<FakeLaunch> {
        Supervisor::new(Config::default(), FakeLaunch(s.clone(), 0), 7)
    }

    #[test]
    fn lazy_start_then_ready() {
        let s = Script::default();
        let mut v = sup(&s);
        let t0 = Instant::now();
        assert_eq!(v.state(), State::Stopped);
        v.ensure_ready(t0, &CancelToken::new()).unwrap();
        assert_eq!((v.state(), v.pid()), (State::Ready, Some(1)));
        v.ensure_ready(t0, &CancelToken::new()).unwrap();
        assert_eq!(v.counters.starts, 1, "no duplicate sidecar");
    }

    #[test]
    fn crash_restarts_after_backoff_and_backoff_grows() {
        let s = Script::default();
        let mut v = sup(&s);
        let t0 = Instant::now();
        v.ensure_ready(t0, &CancelToken::new()).unwrap();
        *s.exited.lock().unwrap() = Some(Some(3));
        v.tick(t0 + Duration::from_secs(1));
        assert_eq!(v.state(), State::Backoff);
        assert_eq!(*s.live.lock().unwrap(), 0, "crashed process released");
        let Err(Unavailable::Backoff { retry_in_ms }) =
            v.ensure_ready(t0 + Duration::from_secs(1), &CancelToken::new())
        else {
            panic!()
        };
        assert!((1600..=2400).contains(&retry_in_ms), "{retry_in_ms}");
        v.ensure_ready(t0 + Duration::from_secs(4), &CancelToken::new())
            .unwrap();
        assert_eq!(v.pid(), Some(2));
        // Second crash: roughly doubled delay.
        *s.exited.lock().unwrap() = Some(None);
        v.tick(t0 + Duration::from_secs(5));
        let Err(Unavailable::Backoff { retry_in_ms }) =
            v.ensure_ready(t0 + Duration::from_secs(5), &CancelToken::new())
        else {
            panic!()
        };
        assert!((3200..=4800).contains(&retry_in_ms), "{retry_in_ms}");
    }

    #[test]
    fn backoff_is_capped_with_jitter() {
        let s = Script::default();
        let mut v = sup(&s);
        for a in 0..40 {
            let d = v.backoff_delay(a);
            assert!(d <= Duration::from_secs(72), "{a}: {d:?}");
        }
        let d = v.backoff_delay(10);
        assert!(d >= Duration::from_secs(48));
    }

    #[test]
    fn restart_budget_exhausts_then_recovers_after_window() {
        let s = Script::default();
        for _ in 0..10 {
            s.launches.lock().unwrap().push_back(Err(LaunchError::Exited(Some(1))));
        }
        let mut v = sup(&s);
        let mut t = Instant::now();
        let c = CancelToken::new();
        let mut attempts = 0;
        loop {
            match v.ensure_ready(t, &c) {
                Err(Unavailable::Backoff { retry_in_ms }) => t += Duration::from_millis(retry_in_ms + 1),
                Err(Unavailable::Failed) => break,
                other => panic!("{other:?}"),
            }
            attempts += 1;
            assert!(attempts < 20);
        }
        assert_eq!(v.state(), State::Failed);
        assert_eq!(v.counters.launch_failures, 6, "1 start + 5 restarts");
        assert!(v.events().any(|(_, e)| *e == Event::BudgetExhausted));
        // Still failed shortly after; usable again once the window has passed.
        assert_eq!(
            v.ensure_ready(t + Duration::from_secs(60), &c).unwrap_err(),
            Unavailable::Failed
        );
        s.launches.lock().unwrap().clear();
        v.ensure_ready(t + Duration::from_secs(3601), &c).unwrap();
        assert_eq!(v.state(), State::Ready);
    }

    #[test]
    fn hung_process_is_killed_after_misses() {
        let s = Script::default();
        let mut v = sup(&s);
        let t0 = Instant::now();
        v.ensure_ready(t0, &CancelToken::new()).unwrap();
        *s.healthy.lock().unwrap() = false;
        for i in 1..=3 {
            v.tick(t0 + Duration::from_secs(5 * i));
        }
        assert_eq!(v.state(), State::Backoff);
        assert_eq!(v.counters.hangs, 1);
        assert_eq!(*s.live.lock().unwrap(), 0);
    }

    #[test]
    fn health_checks_are_rate_limited() {
        let s = Script::default();
        let mut v = sup(&s);
        let t0 = Instant::now();
        v.ensure_ready(t0, &CancelToken::new()).unwrap();
        *s.healthy.lock().unwrap() = false;
        for ms in (0..4_000).step_by(100) {
            v.tick(t0 + Duration::from_millis(ms));
        }
        assert_eq!(v.state(), State::Ready, "no check before the interval");
    }

    #[test]
    fn unload_is_not_a_failure_and_restarts_lazily() {
        let s = Script::default();
        let mut v = sup(&s);
        let t0 = Instant::now();
        v.ensure_ready(t0, &CancelToken::new()).unwrap();
        v.unload(t0);
        assert_eq!((v.state(), *s.live.lock().unwrap()), (State::Stopped, 0));
        v.ensure_ready(t0, &CancelToken::new()).unwrap();
        assert_eq!((v.counters.restarts, v.counters.unloads), (0, 1));
    }

    #[test]
    fn stable_period_resets_backoff() {
        let s = Script::default();
        let mut v = sup(&s);
        let t0 = Instant::now();
        v.ensure_ready(t0, &CancelToken::new()).unwrap();
        *s.exited.lock().unwrap() = Some(None);
        v.tick(t0);
        let t1 = t0 + Duration::from_secs(3);
        v.ensure_ready(t1, &CancelToken::new()).unwrap();
        assert_eq!(v.attempt, 1);
        v.tick(t1 + Duration::from_secs(601));
        assert_eq!(v.attempt, 0, "healthy for 10 min resets the backoff");
    }

    #[test]
    fn cancelled_caller_does_not_start_anything() {
        let s = Script::default();
        let mut v = sup(&s);
        let c = CancelToken::new();
        c.cancel(crate::cancel::CancelReason::Shutdown);
        assert_eq!(v.ensure_ready(Instant::now(), &c).unwrap_err(), Unavailable::Cancelled);
        assert_eq!(v.counters.starts, 0);
    }
}
