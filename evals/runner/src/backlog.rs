//! Backlog drain (Stage 2 criterion: a 2-hour synthetic workload is fully processed within
//! 30 minutes after it ends).
//!
//! A discrete-time simulation (100 ms steps) that drives the **real** `SchedulerCore` — priorities,
//! preemption, fairness, retries, deferrals, gates — with arrivals and service times drawn from a
//! fixed-seed generator. What is simulated is the *cost* of each job: service times are
//! assumptions (below), because no model has been run on this machine. The result therefore
//! shows that the scheduling policy drains this workload under these costs; it does **not**
//! prove real model throughput. Re-run with measured costs once a model is benchmarked
//! (`bench/run_bench.py` latencies → `Profile`).
//!
//! Default profile ("gpu-4b"): plan §5.6 upper bound of 40 window-session flushes per hour; every
//! flush = one embedding job (CPU, 0.4 s) + one extraction job (P1, log-normal, median 8 s =
//! the Stage 0 p50 target, σ 0.5); 10 % of flushes add a VLM job (P1, median 12 s); an Ask (P0,
//! median 5 s) every 10 min on average; one rollup (P2, 25 s) per hour; 20 s cold start; a
//! 10-minute fullscreen period (no P1/P2 model work) and one sidecar crash with a 30 s restart.
//! Two stress profiles are reported alongside: 2× arrival rate, and a CPU-only machine.

use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tsr_infer::sched::{Config, Gates, JobSpec, Outcome, Priority, Resource, SchedulerCore};

#[derive(Clone, Debug, serde::Serialize)]
pub struct Profile {
    pub name: &'static str,
    pub flushes_per_hour: f64,
    pub vlm_fraction: f64,
    pub ask_every_min: f64,
    pub rollup_every_min: f64,
    pub extract_median_s: f64,
    pub vlm_median_s: f64,
    pub ask_median_s: f64,
    pub rollup_s: f64,
    pub embed_s: f64,
    pub sigma: f64,
    pub cold_start_s: f64,
    pub fullscreen_min: (f64, f64),
    pub crash_at_min: f64,
    pub restart_s: f64,
}

pub fn gpu_profile() -> Profile {
    Profile {
        name: "gpu-4b (default)",
        flushes_per_hour: 40.0,
        vlm_fraction: 0.10,
        ask_every_min: 10.0,
        rollup_every_min: 60.0,
        extract_median_s: 8.0,
        vlm_median_s: 12.0,
        ask_median_s: 5.0,
        rollup_s: 25.0,
        embed_s: 0.4,
        sigma: 0.5,
        cold_start_s: 20.0,
        fullscreen_min: (40.0, 50.0),
        crash_at_min: 70.0,
        restart_s: 30.0,
    }
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
    fn exp(&mut self, mean: f64) -> f64 {
        -mean * (1.0 - self.next()).ln()
    }
    fn lognormal(&mut self, median: f64, sigma: f64) -> f64 {
        let (u1, u2) = (self.next().max(1e-12), self.next());
        let z = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
        median * (sigma * z).exp()
    }
}

#[derive(Debug, serde::Serialize)]
pub struct SimResult {
    pub profile: Profile,
    pub jobs: usize,
    pub completed: usize,
    pub failed: u64,
    pub drained_after_end_s: f64,
    pub max_queue_depth: usize,
    pub queue_depth_per_min: Vec<usize>,
    pub preemptions: u64,
    pub deferrals: u64,
    pub retries: u64,
    pub p0_wait_p95_ms: Option<u64>,
    pub p1_wait_p95_ms: Option<u64>,
    pub model_busy_fraction: f64,
}

pub fn simulate(p: &Profile, seed: u64) -> SimResult {
    let mut rng = Rng(seed | 1);
    let window_s = 2.0 * 3600.0;
    // Arrivals: (time_s, spec, cost_s)
    let mut arrivals: Vec<(f64, JobSpec, f64)> = Vec::new();
    let mut n = 0i64;
    let mut job = |kind: &'static str, pr: Priority, r: Resource| {
        n += 1;
        JobSpec {
            key: format!("{kind}:{n}"),
            kind,
            priority: pr,
            resource: r,
            payload: n,
        }
    };
    let mut t = rng.exp(3600.0 / p.flushes_per_hour);
    while t < window_s {
        arrivals.push((t, job("embed", Priority::P1, Resource::Embedder), p.embed_s));
        arrivals.push((
            t,
            job("extract", Priority::P1, Resource::Model),
            rng.lognormal(p.extract_median_s, p.sigma),
        ));
        if rng.next() < p.vlm_fraction {
            arrivals.push((
                t,
                job("vlm", Priority::P1, Resource::Model),
                rng.lognormal(p.vlm_median_s, p.sigma),
            ));
        }
        t += rng.exp(3600.0 / p.flushes_per_hour);
    }
    let mut t = rng.exp(p.ask_every_min * 60.0);
    while t < window_s {
        arrivals.push((
            t,
            job("ask", Priority::P0, Resource::Model),
            rng.lognormal(p.ask_median_s, p.sigma),
        ));
        t += rng.exp(p.ask_every_min * 60.0);
    }
    let mut t = p.rollup_every_min * 60.0;
    while t <= window_s {
        arrivals.push((t, job("rollup", Priority::P2, Resource::Model), p.rollup_s));
        t += p.rollup_every_min * 60.0;
    }
    arrivals.sort_by(|a, b| a.0.total_cmp(&b.0));
    let costs: std::collections::HashMap<String, f64> = arrivals.iter().map(|(_, s, c)| (s.key.clone(), *c)).collect();

    let base = Instant::now();
    let at = |s: f64| base + Duration::from_millis((s * 1000.0) as u64);
    let mut core = SchedulerCore::new(Config::default());
    let mut running: [Option<(u64, f64)>; 2] = [None, None]; // (id, end_s) for Model, Embedder
    let mut loaded = false;
    let mut next = 0;
    let mut done = 0usize;
    let mut last_done_s = 0.0f64;
    let mut depth_per_min = Vec::new();
    let mut max_depth = 0;
    let mut busy = 0.0f64;
    let step = 0.1;
    let mut s = 0.0f64;
    let crash_s = p.crash_at_min * 60.0;
    let mut crashed = false;
    while s < window_s + 6.0 * 3600.0 {
        let now = at(s);
        while next < arrivals.len() && arrivals[next].0 <= s {
            let _ = core.submit(arrivals[next].1.clone(), at(arrivals[next].0));
            next += 1;
        }
        let gates = Gates {
            model_allowed: !(s >= p.fullscreen_min.0 * 60.0 && s < p.fullscreen_min.1 * 60.0),
            background_allowed: true,
        };
        let restarting = s >= crash_s && s < crash_s + p.restart_s;
        // Crash: the running model request fails (retryable); the process restarts.
        if !crashed && s >= crash_s {
            crashed = true;
            loaded = false;
            if let Some((id, _)) = running[0].take() {
                core.finish(id, Outcome::Failed { retryable: true }, now);
            }
        }
        for (ri, r) in [Resource::Model, Resource::Embedder].into_iter().enumerate() {
            if let Some((id, end)) = running[ri]
                && end <= s
            {
                core.finish(id, Outcome::Done, now);
                running[ri] = None;
                done += 1;
                last_done_s = s;
            }
            if let Some((id, _)) = running[ri]
                && core.preempt_target(r, now, &gates) == Some(id)
            {
                // Preempted at a safe boundary: the work so far is discarded.
                core.finish(id, Outcome::Preempted, now);
                running[ri] = None;
            }
            if running[ri].is_none()
                && let Some(d) = core.next(r, now, &gates)
            {
                if r == Resource::Model && restarting {
                    core.finish(d.id, Outcome::Deferred { retry_in_ms: 2_000 }, now);
                    continue;
                }
                let mut cost = costs[&d.spec.key];
                if r == Resource::Model && !loaded {
                    cost += p.cold_start_s;
                    loaded = true;
                }
                running[ri] = Some((d.id, s + cost));
            }
        }
        if running[0].is_some() {
            busy += step;
        }
        let depth = core.queued_len();
        max_depth = max_depth.max(depth);
        if (s / 60.0).fract() < step / 60.0 {
            depth_per_min.push(depth);
        }
        if next == arrivals.len()
            && core.idle(Resource::Model)
            && core.idle(Resource::Embedder)
            && running.iter().all(Option::is_none)
        {
            break;
        }
        s += step;
    }
    let m = &core.metrics;
    SimResult {
        profile: p.clone(),
        jobs: arrivals.len(),
        completed: done,
        failed: m.failed,
        drained_after_end_s: (last_done_s - window_s).max(0.0),
        max_queue_depth: max_depth,
        queue_depth_per_min: depth_per_min,
        preemptions: m.preemptions,
        deferrals: m.deferrals,
        retries: m.retries,
        p0_wait_p95_ms: m.wait_ms[0].quantile_ms(0.95),
        p1_wait_p95_ms: m.wait_ms[1].quantile_ms(0.95),
        model_busy_fraction: busy / last_done_s.max(1.0),
    }
}

pub fn run() -> (bool, Value, String) {
    let base = gpu_profile();
    let stress = Profile {
        name: "2x arrivals (stress)",
        flushes_per_hour: 80.0,
        ask_every_min: 5.0,
        ..gpu_profile()
    };
    let cpu = Profile {
        name: "cpu-only (slow mode)",
        extract_median_s: 45.0,
        vlm_median_s: 90.0,
        ask_median_s: 25.0,
        rollup_s: 120.0,
        cold_start_s: 15.0,
        ..gpu_profile()
    };
    let runs: Vec<SimResult> = [&base, &stress, &cpu].iter().map(|p| simulate(p, 2026)).collect();
    let ok = |r: &SimResult| r.completed == r.jobs && r.failed == 0 && r.drained_after_end_s <= 1800.0;
    let pass = ok(&runs[0]);
    let mut md = String::from(
        "# Backlog drain — SIMULATED costs, real scheduler\n\nThe scheduler is the production `SchedulerCore`; job \
         costs are **assumptions** (Stage 0 targets), not measurements — no model has run on this machine. \
         Criterion: everything from a 2-hour workload done ≤ 30 min after arrivals stop (default profile).\n\n\
         | profile | jobs | completed | failed | drained after end | max queue | preemptions | deferrals | retries | P0 wait p95 | model busy | verdict |\n\
         |---|---|---|---|---|---|---|---|---|---|---|---|\n",
    );
    for r in &runs {
        md.push_str(&format!(
            "| {} | {} | {} | {} | {:.0} s | {} | {} | {} | {} | ≤ {} ms | {:.0} % | {} |\n",
            r.profile.name,
            r.jobs,
            r.completed,
            r.failed,
            r.drained_after_end_s,
            r.max_queue_depth,
            r.preemptions,
            r.deferrals,
            r.retries,
            r.p0_wait_p95_ms.unwrap_or(0),
            r.model_busy_fraction * 100.0,
            if ok(r) { "drains" } else { "does NOT drain in 30 min" }
        ));
    }
    md.push_str("\nAssumptions per profile are in the JSON (`profile`). The CPU-only row is informational: the plan runs extraction on CPU-only machines only on AC power when idle.\n");
    (pass, json!({"runs": runs, "pass": pass}), md)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simulation_is_deterministic() {
        let a = simulate(&gpu_profile(), 7);
        let b = simulate(&gpu_profile(), 7);
        assert_eq!(
            (a.jobs, a.completed, a.preemptions),
            (b.jobs, b.completed, b.preemptions)
        );
    }
}
