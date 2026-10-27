//! Content-free metrics for the diagnostics page (plan §7.4, §6.6): counts, durations, sizes.
//! Nothing here can hold text — keys are fixed `&'static str` job kinds, values are numbers.

use std::collections::BTreeMap;

use serde::Serialize;

/// Upper bucket bounds in milliseconds; the last bucket is everything above.
pub const BUCKETS_MS: [u64; 14] = [
    5, 10, 25, 50, 100, 250, 500, 1_000, 2_500, 5_000, 10_000, 30_000, 60_000, 300_000,
];

#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
pub struct Histogram {
    pub count: u64,
    pub sum_ms: u64,
    pub max_ms: u64,
    pub buckets: [u64; BUCKETS_MS.len() + 1],
}

impl Histogram {
    pub fn record(&mut self, ms: u64) {
        self.count += 1;
        self.sum_ms = self.sum_ms.saturating_add(ms);
        self.max_ms = self.max_ms.max(ms);
        let i = BUCKETS_MS.iter().position(|&b| ms <= b).unwrap_or(BUCKETS_MS.len());
        self.buckets[i] += 1;
    }

    /// Upper bound of the bucket holding quantile `q` (0..=1); `None` when empty.
    pub fn quantile_ms(&self, q: f64) -> Option<u64> {
        if self.count == 0 {
            return None;
        }
        let target = ((self.count as f64) * q.clamp(0.0, 1.0)).ceil().max(1.0) as u64;
        let mut seen = 0;
        for (i, n) in self.buckets.iter().enumerate() {
            seen += n;
            if seen >= target {
                return Some(BUCKETS_MS.get(i).copied().unwrap_or(self.max_ms));
            }
        }
        Some(self.max_ms)
    }

    pub fn mean_ms(&self) -> Option<f64> {
        (self.count > 0).then(|| self.sum_ms as f64 / self.count as f64)
    }
}

/// Scheduler counters. Indexed by priority (P0, P1, P2) where it is an array.
#[derive(Clone, Debug, Default, Serialize)]
pub struct QueueMetrics {
    pub submitted: [u64; 3],
    pub completed: [u64; 3],
    pub dispatched: u64,
    pub failed: u64,
    pub retries: u64,
    pub cancelled: u64,
    pub preemptions: u64,
    pub deferrals: u64,
    pub duplicates: u64,
    pub rejected_full: u64,
    pub fairness_grants: u64,
    /// Time from submit to dispatch, per priority.
    pub wait_ms: [Histogram; 3],
    /// Time a job held its resource, per job kind.
    pub run_ms: BTreeMap<&'static str, Histogram>,
    /// Time from first submit to completion, per job kind.
    pub latency_ms: BTreeMap<&'static str, Histogram>,
}

/// Resource use of a process (the sidecar), where the OS reports it.
#[derive(Clone, Copy, Debug, Default, Serialize, PartialEq, Eq)]
pub struct ProcessUsage {
    pub working_set_mib: Option<u64>,
    pub cpu_ms: Option<u64>,
}

/// One snapshot for the diagnostics page.
#[derive(Clone, Debug, Serialize)]
pub struct Snapshot {
    pub queue_depth: [usize; 3],
    pub oldest_queued_ms: [u64; 3],
    pub queue: QueueMetrics,
    pub sidecar_state: crate::supervisor::State,
    pub sidecar: crate::supervisor::Counters,
    pub sidecar_process: ProcessUsage,
    /// Model requests in flight right now (0 or 1). With nothing queued this is 0, which is what
    /// "no application-issued GPU work" means; overall GPU utilisation is the OS's number and
    /// includes the desktop, the browser and other apps.
    pub model_requests_active: u32,
    pub model_requests_total: u64,
    pub embed_requests_total: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn histogram_quantiles_use_bucket_bounds() {
        let mut h = Histogram::default();
        assert_eq!(h.quantile_ms(0.5), None);
        for ms in [1, 2, 3, 40, 900, 7000] {
            h.record(ms);
        }
        assert_eq!(h.count, 6);
        assert_eq!(h.quantile_ms(0.5), Some(5));
        assert_eq!(h.quantile_ms(1.0), Some(10_000));
        assert_eq!(h.max_ms, 7000);
        h.record(10_000_000);
        assert_eq!(h.quantile_ms(1.0), Some(10_000_000), "overflow bucket reports the max");
    }
}
