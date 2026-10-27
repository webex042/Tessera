//! Threaded runtime (scheduler + supervisor + idle unload) against the SIMULATED sidecar
//! `tsr-fake-llama`, a real separate process. Timings are shortened (backoff, health interval,
//! idle timeout) so each scenario takes seconds; the logic is the production code.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tsr_core::Sensitive;
use tsr_infer::cancel::{CancelReason, CancelToken};
use tsr_infer::client::{ChatModel, ChatRequest, ClientError};
use tsr_infer::http::HttpError;
use tsr_infer::runtime::{Completion, IdlePolicy, Runner, Runtime};
use tsr_infer::sched::{Config as SchedConfig, Dispatch, JobSpec, Outcome, Priority, Resource};
use tsr_infer::sidecar::SidecarSpec;
use tsr_infer::supervisor::{Config as SupConfig, SpecLauncher, State, Supervisor};

fn spec(extra: &[&str]) -> SidecarSpec {
    SidecarSpec {
        server_exe: PathBuf::from(env!("CARGO_BIN_EXE_tsr-fake-llama")),
        model: None,
        mmproj: None,
        device_args: vec![],
        ctx: 2048,
        threads: Some(1),
        extra_args: extra.iter().map(|s| s.to_string()).collect(),
        extra_env: vec![],
        ready_timeout: Duration::from_secs(10),
    }
}

fn fast_sup() -> SupConfig {
    SupConfig {
        health_interval: Duration::from_millis(200),
        health_timeout: Duration::from_millis(300),
        misses_before_restart: 2,
        backoff_initial: Duration::from_millis(100),
        backoff_max: Duration::from_millis(400),
        max_restarts: 3,
        ..SupConfig::default()
    }
}

/// Calls the model like a real job: streaming for P1/P2 (preemptible), plain for P0. Records the
/// order of completed jobs and the maximum number of concurrent model calls.
#[derive(Clone, Default)]
struct Probe {
    done: Arc<Mutex<Vec<String>>>,
    active: Arc<AtomicU32>,
    max_active: Arc<AtomicU32>,
}

struct ModelRunner(Probe);

impl Runner for ModelRunner {
    fn run(&mut self, job: &Dispatch, model: Option<&dyn ChatModel>, cancel: &CancelToken) -> Outcome {
        let model = model.expect("model jobs get a client");
        let n = self.0.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.0.max_active.fetch_max(n, Ordering::SeqCst);
        let r = model.chat(
            &ChatRequest {
                system: Sensitive::new("sys".into()),
                user: Sensitive::new(job.spec.key.clone()),
                image_png: None,
                temperature: 0.0,
                max_tokens: 32,
                json_schema: None,
                stream: job.spec.priority != Priority::P0,
                seed: None,
            },
            cancel,
        );
        self.0.active.fetch_sub(1, Ordering::SeqCst);
        match r {
            Ok(_) => {
                self.0.done.lock().unwrap().push(job.spec.key.clone());
                Outcome::Done
            }
            Err(ClientError::Http(HttpError::Cancelled(CancelReason::Preempted))) => Outcome::Preempted,
            Err(ClientError::Http(HttpError::Cancelled(_))) => Outcome::Cancelled,
            Err(_) => Outcome::Failed { retryable: true },
        }
    }
}

struct EmbedRunner;
impl Runner for EmbedRunner {
    fn run(&mut self, _: &Dispatch, model: Option<&dyn ChatModel>, _: &CancelToken) -> Outcome {
        assert!(model.is_none(), "embedder jobs never get the model");
        Outcome::Done
    }
}

fn rt(extra: &[&str], idle: IdlePolicy, probe: &Probe) -> Runtime<SpecLauncher> {
    Runtime::start(
        SchedConfig {
            retry_delay: Duration::from_millis(100),
            ..SchedConfig::default()
        },
        Supervisor::new(fast_sup(), SpecLauncher(spec(extra)), 42),
        idle,
        Box::new(ModelRunner(probe.clone())),
        Box::new(EmbedRunner),
    )
}

fn job(key: &str, p: Priority) -> JobSpec {
    JobSpec {
        key: key.into(),
        kind: "test",
        priority: p,
        resource: Resource::Model,
        payload: 0,
    }
}

/// Collect completions until `n` jobs have *finished* (Done, Failed or Cancelled). A preemption
/// is reported too but the job runs again, so it does not count towards `n`.
fn wait_for_final(rx: &Receiver<Completion>, n: usize, secs: u64) -> Vec<Completion> {
    let end = Instant::now() + Duration::from_secs(secs);
    let mut out: Vec<Completion> = Vec::new();
    while out.iter().filter(|c| c.outcome != Outcome::Preempted).count() < n && Instant::now() < end {
        if let Ok(c) = rx.recv_timeout(Duration::from_millis(50)) {
            out.push(c);
        }
    }
    out
}

fn wait_for(rx: &Receiver<Completion>, n: usize, secs: u64) -> Vec<Completion> {
    let end = Instant::now() + Duration::from_secs(secs);
    let mut out = Vec::new();
    while out.len() < n && Instant::now() < end {
        if let Ok(c) = rx.recv_timeout(Duration::from_millis(50)) {
            out.push(c);
        }
    }
    out
}

const NEVER: IdlePolicy = IdlePolicy {
    on_ac: None,
    on_battery: None,
};

#[test]
fn p0_preempts_streaming_p1_and_p1_completes_after() {
    let probe = Probe::default();
    let r = rt(&["--fake-token-ms", "40", "--fake-tokens", "50"], NEVER, &probe);
    let rx = r.subscribe();
    r.submit(job("extract-1", Priority::P1)).unwrap();
    std::thread::sleep(Duration::from_millis(700)); // sidecar start + some tokens
    r.submit(job("ask", Priority::P0)).unwrap();
    let c = wait_for(&rx, 3, 20);
    let outcomes: Vec<(String, Outcome)> = c.iter().map(|c| (c.key.clone(), c.outcome)).collect();
    println!("SIMULATED preemption sequence: {outcomes:?}");
    assert_eq!(
        outcomes,
        vec![
            ("extract-1".into(), Outcome::Preempted),
            ("ask".into(), Outcome::Done),
            ("extract-1".into(), Outcome::Done)
        ]
    );
    assert_eq!(
        *probe.done.lock().unwrap(),
        vec!["ask", "extract-1"],
        "partial P1 output never committed"
    );
    let s = r.snapshot();
    assert_eq!(s.queue.preemptions, 1);
    assert_eq!(s.queue.completed, [1, 1, 0]);
    r.shutdown();
}

#[test]
fn crash_is_detected_and_sidecar_restarts() {
    let probe = Probe::default();
    let r = rt(&["--fake-exit-after", "1"], NEVER, &probe);
    let rx = r.subscribe();
    r.submit(job("a", Priority::P1)).unwrap();
    assert_eq!(wait_for(&rx, 1, 15)[0].outcome, Outcome::Done);
    let first_pid = r.sidecar_pid();
    // The fake exits after its first request; the monitor notices, backs off, and the next job
    // runs on a new process.
    std::thread::sleep(Duration::from_millis(500));
    r.submit(job("b", Priority::P1)).unwrap();
    let c = wait_for(&rx, 1, 15);
    assert_eq!(c[0].outcome, Outcome::Done);
    let s = r.snapshot();
    println!("SIMULATED crash recovery counters: {:?}", s.sidecar);
    assert!(s.sidecar.crashes >= 1 && s.sidecar.restarts >= 1);
    assert_ne!(r.sidecar_pid(), first_pid);
    r.shutdown();
}

#[test]
fn hung_sidecar_is_killed_and_replaced() {
    let probe = Probe::default();
    let r = rt(&["--fake-hang-after", "1"], NEVER, &probe);
    let rx = r.subscribe();
    r.submit(job("a", Priority::P0)).unwrap();
    assert_eq!(wait_for(&rx, 1, 15)[0].outcome, Outcome::Done);
    let hung_pid = r.sidecar_pid().unwrap();
    // Health checks now hang; after two misses the process is killed.
    let end = Instant::now() + Duration::from_secs(10);
    while r.snapshot().sidecar.hangs == 0 && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(100));
    }
    assert_eq!(r.snapshot().sidecar.hangs, 1);
    #[cfg(windows)]
    assert!(
        tsr_win::job::wait_process_gone(hung_pid, 5_000),
        "hung process terminated"
    );
    r.shutdown();
}

#[test]
fn restart_budget_exhaustion_fails_interactive_jobs_and_defers_background() {
    let probe = Probe::default();
    let r = rt(&["--fake-crash-on-start"], NEVER, &probe);
    let rx = r.subscribe();
    r.submit(job("ask", Priority::P0)).unwrap();
    let c = wait_for(&rx, 1, 20);
    assert_eq!(c[0].outcome, Outcome::Failed { retryable: false });
    assert_eq!(r.sidecar_state(), State::Failed);
    r.submit(job("bg", Priority::P1)).unwrap();
    std::thread::sleep(Duration::from_millis(500));
    let s = r.snapshot();
    assert_eq!(
        s.queue_depth[1], 1,
        "background job stays queued while the model is unavailable"
    );
    assert!(s.sidecar.launch_failures >= 4);
    r.shutdown();
}

#[test]
fn idle_unload_on_battery_then_lazy_restart() {
    let probe = Probe::default();
    let r = rt(
        &[],
        IdlePolicy {
            on_ac: None,
            on_battery: Some(Duration::from_millis(600)),
        },
        &probe,
    );
    let rx = r.subscribe();
    r.set_on_battery(true);
    r.submit(job("a", Priority::P1)).unwrap();
    wait_for(&rx, 1, 15);
    let pid = r.sidecar_pid().expect("loaded");
    std::thread::sleep(Duration::from_millis(1_500));
    assert_eq!(r.sidecar_pid(), None, "unloaded after the battery idle timeout");
    #[cfg(windows)]
    assert!(tsr_win::job::wait_process_gone(pid, 5_000));
    let s = r.snapshot();
    assert_eq!(
        (s.sidecar.unloads, s.sidecar.restarts, s.model_requests_active),
        (1, 0, 0)
    );
    // A request after unload starts it again.
    r.submit(job("b", Priority::P0)).unwrap();
    assert_eq!(wait_for(&rx, 1, 15)[0].outcome, Outcome::Done);
    assert_eq!(r.snapshot().sidecar.starts, 2);
    r.shutdown();
}

#[test]
fn on_ac_the_model_stays_loaded() {
    let probe = Probe::default();
    let r = rt(&[], IdlePolicy::default(), &probe);
    let rx = r.subscribe();
    r.set_on_battery(false);
    r.submit(job("a", Priority::P1)).unwrap();
    wait_for(&rx, 1, 15);
    std::thread::sleep(Duration::from_millis(1_000));
    assert!(r.sidecar_pid().is_some());
    r.shutdown();
}

#[test]
fn request_arriving_during_unload_is_served() {
    let probe = Probe::default();
    // One-token replies: Windows sleeps in ~15.6 ms ticks, so 20 tokens would take ~310 ms.
    let r = rt(
        &["--fake-tokens", "1"],
        IdlePolicy {
            on_ac: Some(Duration::from_millis(150)),
            on_battery: Some(Duration::from_millis(150)),
        },
        &probe,
    );
    let rx = r.subscribe();
    // Keep submitting around the unload instant; every job must complete exactly once.
    for i in 0..12 {
        r.submit(job(&format!("j{i}"), Priority::P1)).unwrap();
        // Mostly back-to-back, with pauses longer than the idle timeout every third job, so
        // unloads and new submissions interleave.
        std::thread::sleep(Duration::from_millis(if i % 3 == 2 { 700 } else { 40 }));
    }
    let c = wait_for(&rx, 12, 30);
    assert_eq!(c.len(), 12);
    assert!(c.iter().all(|c| c.outcome == Outcome::Done));
    // On a loaded machine the sidecar start can use up the pauses above, so no unload may have
    // happened yet. The idle policy guarantees one soon: wait for it, then a request that
    // arrives after the unload must still be served (found flaky under load, 2026-10-28).
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while r.snapshot().sidecar.unloads == 0 && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    let s = r.snapshot();
    println!(
        "SIMULATED unload race: starts={} unloads={}",
        s.sidecar.starts, s.sidecar.unloads
    );
    assert!(s.sidecar.unloads >= 1, "the idle policy unloads the model");
    r.submit(job("after-unload", Priority::P1)).unwrap();
    let c = wait_for(&rx, 1, 30);
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].outcome, Outcome::Done, "a request after an unload is served");
    r.shutdown();
}

#[test]
fn concurrent_submissions_complete_exactly_once_one_at_a_time() {
    let probe = Probe::default();
    let r = Arc::new(rt(&["--fake-token-ms", "1"], NEVER, &probe));
    let rx = r.subscribe();
    let mut hs = vec![];
    for t in 0..4 {
        let r = r.clone();
        hs.push(std::thread::spawn(move || {
            for i in 0..6 {
                let p = [Priority::P0, Priority::P1, Priority::P2][(t + i) % 3];
                r.submit(job(&format!("t{t}-{i}"), p)).unwrap();
                // Duplicate submit of the same key is refused, never run twice.
                assert!(r.submit(job(&format!("t{t}-{i}"), p)).is_err());
            }
        }));
    }
    for h in hs {
        h.join().unwrap();
    }
    // Preemptions are allowed here (P0 jobs arrive while P1/P2 run); each job must still finish
    // as Done exactly once.
    let c = wait_for_final(&rx, 24, 60);
    let done: Vec<_> = c
        .iter()
        .filter(|c| c.outcome == Outcome::Done)
        .map(|c| c.key.clone())
        .collect();
    let unique: HashSet<_> = done.iter().collect();
    assert_eq!((done.len(), unique.len()), (24, 24), "{c:?}");
    assert_eq!(
        probe.max_active.load(Ordering::SeqCst),
        1,
        "one model request in flight"
    );
    match Arc::try_unwrap(r) {
        Ok(r) => r.shutdown(),
        Err(_) => panic!("runtime still shared"),
    }
}

#[test]
fn running_job_can_be_cancelled() {
    let probe = Probe::default();
    let r = rt(&["--fake-token-ms", "50", "--fake-tokens", "100"], NEVER, &probe);
    let rx = r.subscribe();
    let id = r.submit(job("long", Priority::P1)).unwrap();
    std::thread::sleep(Duration::from_millis(800));
    r.cancel(id);
    let c = wait_for(&rx, 1, 10);
    assert_eq!(c[0].outcome, Outcome::Cancelled);
    assert!(probe.done.lock().unwrap().is_empty());
    r.shutdown();
}

#[test]
fn no_model_requests_when_nothing_is_queued_and_shutdown_kills_sidecar() {
    let probe = Probe::default();
    let r = rt(&[], NEVER, &probe);
    let rx = r.subscribe();
    r.submit(job("a", Priority::P1)).unwrap();
    wait_for(&rx, 1, 15);
    let before = r.snapshot().model_requests_total;
    std::thread::sleep(Duration::from_millis(1_200));
    let s = r.snapshot();
    assert_eq!((s.model_requests_total, s.model_requests_active), (before, 0));
    // Embedder work never starts or touches the model.
    let mut e = job("embed-1", Priority::P1);
    e.resource = Resource::Embedder;
    r.submit(e).unwrap();
    wait_for(&rx, 1, 5);
    assert_eq!(r.snapshot().model_requests_total, before);
    let pid = r.sidecar_pid().unwrap();
    r.shutdown();
    #[cfg(windows)]
    assert!(
        tsr_win::job::wait_process_gone(pid, 5_000),
        "sidecar gone after shutdown"
    );
}
