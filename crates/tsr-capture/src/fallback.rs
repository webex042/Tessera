//! Per-window fallback from UIA (rung 1) to OCR (rung 2) (plan §5.4).
//!
//! A window whose UIA read hits the deadline twice in a row is read with rung 2 only for
//! `cooldown` (10 minutes by default); then UIA is tried again. A successful read resets the
//! count. `Busy` (the worker was still stuck on *another* window) and ordinary failures do not
//! count against a window. Times are passed in so the policy is testable without waiting.

use std::collections::HashMap;
use std::hash::Hash;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiaOutcome {
    Ok,
    TimedOut,
    Busy,
    Failed,
}

#[derive(Debug, Clone, Copy)]
struct Entry {
    strikes: u32,
    cooling_until: Option<Instant>,
    last_seen: Instant,
}

pub struct UiaFallback<K> {
    max_strikes: u32,
    cooldown: Duration,
    capacity: usize,
    entries: HashMap<K, Entry>,
}

impl<K: Eq + Hash + Clone> Default for UiaFallback<K> {
    fn default() -> Self {
        Self::new(2, Duration::from_secs(600), 512)
    }
}

impl<K: Eq + Hash + Clone> UiaFallback<K> {
    pub fn new(max_strikes: u32, cooldown: Duration, capacity: usize) -> Self {
        UiaFallback {
            max_strikes,
            cooldown,
            capacity,
            entries: HashMap::new(),
        }
    }

    /// Should rung 1 be tried for this window now?
    pub fn allow_uia(&mut self, key: &K, now: Instant) -> bool {
        match self.entries.get_mut(key) {
            Some(e) => match e.cooling_until {
                Some(t) if now < t => false,
                Some(_) => {
                    e.cooling_until = None;
                    e.strikes = 0;
                    true
                }
                None => true,
            },
            None => true,
        }
    }

    pub fn record(&mut self, key: &K, outcome: UiaOutcome, now: Instant) {
        if !self.entries.contains_key(key) && self.entries.len() >= self.capacity {
            // Forget the window seen longest ago; closed windows never come back.
            if let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, e)| e.last_seen)
                .map(|(k, _)| k.clone())
            {
                self.entries.remove(&oldest);
            }
        }
        let e = self.entries.entry(key.clone()).or_insert(Entry {
            strikes: 0,
            cooling_until: None,
            last_seen: now,
        });
        e.last_seen = now;
        match outcome {
            UiaOutcome::Ok => {
                e.strikes = 0;
                e.cooling_until = None;
            }
            UiaOutcome::TimedOut => {
                e.strikes += 1;
                if e.strikes >= self.max_strikes {
                    e.cooling_until = Some(now + self.cooldown);
                    tracing::info!(
                        strikes = e.strikes,
                        cooldown_s = self.cooldown.as_secs(),
                        "uia: window falls back to OCR"
                    );
                }
            }
            UiaOutcome::Busy | UiaOutcome::Failed => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_timeouts_in_a_row_switch_to_ocr_for_the_cooldown() {
        let mut p = UiaFallback::default();
        let t = Instant::now();
        assert!(p.allow_uia(&1, t));
        p.record(&1, UiaOutcome::TimedOut, t);
        assert!(p.allow_uia(&1, t), "one timeout is not enough");
        p.record(&1, UiaOutcome::TimedOut, t);
        assert!(!p.allow_uia(&1, t + Duration::from_secs(1)));
        assert!(!p.allow_uia(&1, t + Duration::from_secs(599)));
        assert!(
            p.allow_uia(&1, t + Duration::from_secs(601)),
            "UIA retried after the cooldown"
        );
        // Fresh start after the cooldown: one more timeout is not enough again.
        p.record(&1, UiaOutcome::TimedOut, t + Duration::from_secs(601));
        assert!(p.allow_uia(&1, t + Duration::from_secs(602)));
    }

    #[test]
    fn success_resets_and_busy_or_failed_do_not_count() {
        let mut p = UiaFallback::default();
        let t = Instant::now();
        p.record(&7, UiaOutcome::TimedOut, t);
        p.record(&7, UiaOutcome::Ok, t);
        p.record(&7, UiaOutcome::TimedOut, t);
        assert!(p.allow_uia(&7, t));
        p.record(&8, UiaOutcome::Busy, t);
        p.record(&8, UiaOutcome::Busy, t);
        p.record(&8, UiaOutcome::Failed, t);
        assert!(p.allow_uia(&8, t));
        assert!(p.allow_uia(&9, t), "other windows unaffected");
    }

    #[test]
    fn memory_is_bounded() {
        let mut p = UiaFallback::new(2, Duration::from_secs(600), 3);
        let t = Instant::now();
        for k in 0..10u32 {
            p.record(&k, UiaOutcome::TimedOut, t + Duration::from_millis(k as u64));
        }
        assert_eq!(p.entries.len(), 3);
        assert!(p.entries.contains_key(&9));
    }
}
