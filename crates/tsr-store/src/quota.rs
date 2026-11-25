//! Application-level Groq free-tier quota enforcement backed by SQLCipher `settings` (or
//! in-memory state for isolated tests).
//!
//! Safety targets (below Groq's published free-tier ceiling so interactive `P0` Ask/Draft keep
//! headroom and background jobs never churn into 429 loops):
//! - 20 requests / minute, 6,000 tokens / minute
//! - 700 requests / day, 150,000 tokens / day (UTC day)
//! - Background (`P1`/`P2`) defers at >= 70% daily budget or >= 65% minute budget.
//!
//! All reads/writes against SQLCipher use `BEGIN IMMEDIATE` inside an internal mutex so
//! concurrent workers share consistent state without losing reservations or token updates.

use std::sync::Mutex;

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::StoreError;

pub const QUOTA_SETTING_KEY: &str = "groq_quota_v1";
const MINUTE_MS: i64 = 60_000;
const DAY_MS: i64 = 86_400_000;
const MAX_STORED_SAMPLES: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuotaPriority {
    Interactive,
    Background,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QuotaLimits {
    pub rpm: u64,
    pub rpd: u64,
    pub tpm: u64,
    pub tpd: u64,
    pub background_daily_pct: u64,
    pub background_minute_pct: u64,
}

impl Default for QuotaLimits {
    fn default() -> Self {
        QuotaLimits {
            rpm: 20,
            rpd: 700,
            tpm: 6_000,
            tpd: 150_000,
            background_daily_pct: 70,
            background_minute_pct: 65,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum QuotaReject {
    #[error("rate limited; retry after {retry_after_ms} ms")]
    RateLimited { retry_after_ms: u64 },
    #[error("background quota reserve exceeded; retry after {retry_after_ms} ms")]
    BackgroundDeferred { retry_after_ms: u64 },
    #[error("daily quota exhausted")]
    DailyExhausted,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct MinuteSample {
    id: u64,
    at_ms: i64,
    tokens: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
struct QuotaState {
    utc_day: i64,
    day_requests: u64,
    day_tokens: u64,
    next_id: u64,
    samples: Vec<MinuteSample>,
    cooldown_until_ms: i64,
    remote_exhausted_day: Option<i64>,
}

impl QuotaState {
    fn normalize(&mut self, now_ms: i64) {
        let day = now_ms.div_euclid(DAY_MS);
        if self.utc_day != day {
            self.utc_day = day;
            self.day_requests = 0;
            self.day_tokens = 0;
            if self.remote_exhausted_day != Some(day) {
                self.remote_exhausted_day = None;
            }
        }
        let cutoff = now_ms - MINUTE_MS;
        self.samples.retain(|s| s.at_ms > cutoff && s.at_ms <= now_ms);
        if self.samples.len() > MAX_STORED_SAMPLES {
            let drop_n = self.samples.len() - MAX_STORED_SAMPLES;
            self.samples.drain(0..drop_n);
        }
        if self.cooldown_until_ms < now_ms {
            self.cooldown_until_ms = 0;
        }
    }

    fn minute_requests(&self) -> u64 {
        self.samples.len() as u64
    }

    fn minute_tokens(&self) -> u64 {
        self.samples.iter().map(|s| s.tokens).sum()
    }

    fn next_minute_slot_ms(&self, now_ms: i64) -> u64 {
        self.samples
            .iter()
            .map(|s| s.at_ms + MINUTE_MS)
            .min()
            .map(|t| (t - now_ms).max(1_000) as u64)
            .unwrap_or(5_000)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reservation {
    sample_id: u64,
    utc_day: i64,
    reserved_tokens: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct QuotaSnapshot {
    pub utc_day: i64,
    pub day_requests: u64,
    pub day_tokens: u64,
    pub minute_requests: u64,
    pub minute_tokens: u64,
    pub cooldown_ms: u64,
    pub background_throttled: bool,
    pub daily_exhausted: bool,
}

enum Backend {
    Memory(Mutex<QuotaState>),
    Sqlite(Mutex<Connection>),
}

pub struct QuotaTracker {
    limits: QuotaLimits,
    backend: Backend,
}

/// Estimate prompt + bounded response tokens cleanly without calling any remote API.
pub fn estimate_request_tokens(system: &str, user: &str, max_completion_tokens: u32) -> u64 {
    let prompt_chars = system.chars().count() + user.chars().count();
    let est_prompt = (prompt_chars as u64).div_ceil(4).saturating_add(48);
    let est_completion = (max_completion_tokens as u64).min(512);
    est_prompt.saturating_add(est_completion)
}

fn ms_until_next_utc_day(now_ms: i64) -> u64 {
    let day = now_ms.div_euclid(DAY_MS);
    let next = (day + 1) * DAY_MS;
    (next - now_ms).max(60_000) as u64
}

fn pct(limit: u64, percent: u64) -> u64 {
    limit.saturating_mul(percent) / 100
}

fn check_gate(
    state: &QuotaState,
    limits: QuotaLimits,
    priority: QuotaPriority,
    est_tokens: u64,
    now_ms: i64,
) -> Result<(), QuotaReject> {
    let min_reqs = state.minute_requests();
    let min_toks = state.minute_tokens();

    if state.remote_exhausted_day == Some(state.utc_day) {
        return Err(QuotaReject::DailyExhausted);
    }
    if state.cooldown_until_ms > now_ms {
        let retry_after_ms = (state.cooldown_until_ms - now_ms).max(1) as u64;
        return Err(match priority {
            QuotaPriority::Background => QuotaReject::BackgroundDeferred {
                retry_after_ms: retry_after_ms.max(5_000),
            },
            QuotaPriority::Interactive => QuotaReject::RateLimited { retry_after_ms },
        });
    }

    if priority == QuotaPriority::Background {
        let bg_rpd = pct(limits.rpd, limits.background_daily_pct);
        let bg_tpd = pct(limits.tpd, limits.background_daily_pct);
        if state.day_requests >= bg_rpd || state.day_tokens.saturating_add(est_tokens) > bg_tpd {
            return Err(QuotaReject::BackgroundDeferred {
                retry_after_ms: ms_until_next_utc_day(now_ms).min(300_000),
            });
        }
        let bg_rpm = pct(limits.rpm, limits.background_minute_pct);
        let bg_tpm = pct(limits.tpm, limits.background_minute_pct);
        if min_reqs >= bg_rpm || min_toks.saturating_add(est_tokens) > bg_tpm {
            return Err(QuotaReject::BackgroundDeferred {
                retry_after_ms: state.next_minute_slot_ms(now_ms).max(5_000),
            });
        }
    }

    if state.day_requests >= limits.rpd || state.day_tokens.saturating_add(est_tokens) > limits.tpd {
        return Err(QuotaReject::DailyExhausted);
    }
    if min_reqs >= limits.rpm || min_toks.saturating_add(est_tokens) > limits.tpm {
        return Err(QuotaReject::RateLimited {
            retry_after_ms: state.next_minute_slot_ms(now_ms),
        });
    }
    Ok(())
}

impl QuotaTracker {
    pub fn in_memory(limits: QuotaLimits) -> Self {
        QuotaTracker {
            limits,
            backend: Backend::Memory(Mutex::new(QuotaState::default())),
        }
    }

    pub fn open(conn: Connection, limits: QuotaLimits) -> Self {
        QuotaTracker {
            limits,
            backend: Backend::Sqlite(Mutex::new(conn)),
        }
    }

    pub fn limits(&self) -> QuotaLimits {
        self.limits
    }

    fn with_state<T>(&self, f: impl FnOnce(&mut QuotaState) -> T) -> Result<T, StoreError> {
        match &self.backend {
            Backend::Memory(m) => {
                let mut g = m.lock().unwrap_or_else(|p| p.into_inner());
                Ok(f(&mut g))
            }
            Backend::Sqlite(m) => {
                let conn = m.lock().unwrap_or_else(|p| p.into_inner());
                conn.execute_batch("BEGIN IMMEDIATE")?;
                let res = (|| -> Result<T, StoreError> {
                    let existing: Option<String> = conn
                        .query_row("SELECT value FROM settings WHERE key = ?1", [QUOTA_SETTING_KEY], |r| {
                            r.get(0)
                        })
                        .optional()?;
                    let mut st: QuotaState = existing
                        .as_deref()
                        .and_then(|s| serde_json::from_str(s).ok())
                        .unwrap_or_default();
                    let out = f(&mut st);
                    let json = serde_json::to_string(&st).map_err(|_| StoreError::Invalid("quota json"))?;
                    conn.execute(
                        "INSERT INTO settings (key, value) VALUES (?1, ?2)
                         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                        params![QUOTA_SETTING_KEY, json],
                    )?;
                    Ok(out)
                })();
                match res {
                    Ok(v) => {
                        conn.execute_batch("COMMIT")?;
                        Ok(v)
                    }
                    Err(e) => {
                        let _ = conn.execute_batch("ROLLBACK");
                        Err(e)
                    }
                }
            }
        }
    }

    /// Check whether background jobs currently have enough quota reserve without modifying state.
    pub fn background_allowed(&self, now_ms: i64) -> Result<(), QuotaReject> {
        let mut out = Ok(());
        let _ = self.with_state(|st| {
            st.normalize(now_ms);
            out = check_gate(st, self.limits, QuotaPriority::Background, 1, now_ms);
        });
        out
    }

    /// Atomically reserve request + estimated token capacity before sending a Groq API call.
    pub fn try_reserve(
        &self,
        priority: QuotaPriority,
        estimated_tokens: u64,
        now_ms: i64,
    ) -> Result<Reservation, QuotaReject> {
        // Clamp per-call initial estimate so a single max prompt can fit inside TPM unless already high.
        let est = estimated_tokens.max(1);
        let mut result = Err(QuotaReject::DailyExhausted);
        let store_res = self.with_state(|st| {
            st.normalize(now_ms);
            if let Err(rej) = check_gate(st, self.limits, priority, est, now_ms) {
                result = Err(rej);
                return;
            }
            st.next_id = st.next_id.saturating_add(1).max(1);
            let id = st.next_id;
            st.day_requests = st.day_requests.saturating_add(1);
            st.day_tokens = st.day_tokens.saturating_add(est);
            st.samples.push(MinuteSample {
                id,
                at_ms: now_ms,
                tokens: est,
            });
            result = Ok(Reservation {
                sample_id: id,
                utc_day: st.utc_day,
                reserved_tokens: est,
            });
        });
        if store_res.is_err() {
            return Err(QuotaReject::RateLimited { retry_after_ms: 5_000 });
        }
        result
    }

    /// Reconcile a previously reserved slot with real usage and rate-limit headers from Groq.
    pub fn reconcile(
        &self,
        res: Reservation,
        actual_tokens: Option<u64>,
        keep_request: bool,
        cooldown_ms: Option<u64>,
        mark_daily_exhausted: bool,
        now_ms: i64,
    ) {
        let _ = self.with_state(|st| {
            st.normalize(now_ms);
            if st.utc_day == res.utc_day {
                if !keep_request {
                    st.day_requests = st.day_requests.saturating_sub(1);
                }
                let final_tokens = match actual_tokens {
                    Some(t) => t,
                    None if !keep_request => 0,
                    None => res.reserved_tokens,
                };
                if final_tokens >= res.reserved_tokens {
                    st.day_tokens = st.day_tokens.saturating_add(final_tokens - res.reserved_tokens);
                } else {
                    st.day_tokens = st.day_tokens.saturating_sub(res.reserved_tokens - final_tokens);
                }
                if let Some(pos) = st.samples.iter().position(|s| s.id == res.sample_id) {
                    if !keep_request && actual_tokens.is_none() {
                        st.samples.remove(pos);
                    } else {
                        st.samples[pos].tokens = final_tokens;
                    }
                }
            }
            if let Some(ms) = cooldown_ms
                && ms > 0
            {
                let until = now_ms.saturating_add(ms as i64);
                st.cooldown_until_ms = st.cooldown_until_ms.max(until);
            }
            if mark_daily_exhausted {
                st.remote_exhausted_day = Some(st.utc_day);
            }
        });
    }

    pub fn snapshot(&self, now_ms: i64) -> QuotaSnapshot {
        let mut snap = QuotaSnapshot::default();
        let _ = self.with_state(|st| {
            st.normalize(now_ms);
            let bg = check_gate(st, self.limits, QuotaPriority::Background, 1, now_ms);
            snap = QuotaSnapshot {
                utc_day: st.utc_day,
                day_requests: st.day_requests,
                day_tokens: st.day_tokens,
                minute_requests: st.minute_requests(),
                minute_tokens: st.minute_tokens(),
                cooldown_ms: if st.cooldown_until_ms > now_ms {
                    (st.cooldown_until_ms - now_ms) as u64
                } else {
                    0
                },
                background_throttled: matches!(bg, Err(QuotaReject::BackgroundDeferred { .. })),
                daily_exhausted: st.remote_exhausted_day == Some(st.utc_day)
                    || st.day_requests >= self.limits.rpd
                    || st.day_tokens >= self.limits.tpd,
            };
        });
        snap
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrate::migrate;

    fn sqlite_conn() -> Connection {
        let mut c = Connection::open_in_memory().unwrap();
        migrate(&mut c).unwrap();
        c
    }

    #[test]
    fn enforces_minute_and_daily_limits_and_resets() {
        let q = QuotaTracker::in_memory(QuotaLimits {
            rpm: 3,
            rpd: 5,
            tpm: 1_000,
            tpd: 2_000,
            background_daily_pct: 70,
            background_minute_pct: 65,
        });
        let t0 = 1_700_000_000_000i64;
        for i in 0..3 {
            let r = q.try_reserve(QuotaPriority::Interactive, 100, t0 + i * 100).unwrap();
            q.reconcile(r, Some(100), true, None, false, t0 + i * 100);
        }
        assert!(matches!(
            q.try_reserve(QuotaPriority::Interactive, 100, t0 + 1_000),
            Err(QuotaReject::RateLimited { .. })
        ));
        // After 61 seconds the minute window opens up again.
        let t1 = t0 + 61_000;
        let r4 = q.try_reserve(QuotaPriority::Interactive, 100, t1).unwrap();
        q.reconcile(r4, Some(100), true, None, false, t1);
        let r5 = q.try_reserve(QuotaPriority::Interactive, 100, t1 + 100).unwrap();
        q.reconcile(r5, Some(100), true, None, false, t1 + 100);
        assert_eq!(
            q.try_reserve(QuotaPriority::Interactive, 100, t1 + 200),
            Err(QuotaReject::DailyExhausted)
        );
        // Next UTC day resets daily counters cleanly.
        let next_day = t0 + DAY_MS + 1_000;
        assert!(q.try_reserve(QuotaPriority::Interactive, 100, next_day).is_ok());
    }

    #[test]
    fn background_defers_while_interactive_proceeds() {
        let q = QuotaTracker::in_memory(QuotaLimits {
            rpm: 20,
            rpd: 10,
            tpm: 6_000,
            tpd: 10_000,
            background_daily_pct: 70, // 7 reqs / 7000 toks
            background_minute_pct: 65,
        });
        let t0 = 1_700_000_000_000i64;
        for i in 0..7 {
            let r = q.try_reserve(QuotaPriority::Background, 200, t0 + i * 10).unwrap();
            q.reconcile(r, Some(200), true, None, false, t0 + i * 10);
        }
        assert!(matches!(
            q.try_reserve(QuotaPriority::Background, 200, t0 + 100),
            Err(QuotaReject::BackgroundDeferred { .. })
        ));
        // Interactive P0 still retains reserved headroom!
        assert!(q.try_reserve(QuotaPriority::Interactive, 200, t0 + 100).is_ok());
    }

    #[test]
    fn persists_quota_in_sqlite_settings_across_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("q.db");
        let t0 = 1_700_000_000_000i64;
        {
            let mut c = Connection::open(&path).unwrap();
            migrate(&mut c).unwrap();
            let q = QuotaTracker::open(c, QuotaLimits::default());
            let r = q.try_reserve(QuotaPriority::Interactive, 500, t0).unwrap();
            q.reconcile(r, Some(420), true, None, false, t0);
        }
        {
            let c = Connection::open(&path).unwrap();
            let q = QuotaTracker::open(c, QuotaLimits::default());
            let snap = q.snapshot(t0 + 1_000);
            assert_eq!(snap.day_requests, 1);
            assert_eq!(snap.day_tokens, 420);
        }
    }

    #[test]
    fn handles_concurrent_reservations_safely() {
        use std::sync::Arc;
        let _ = sqlite_conn();
        let q = Arc::new(QuotaTracker::in_memory(QuotaLimits {
            rpm: 5,
            rpd: 10,
            tpm: 10_000,
            tpd: 10_000,
            background_daily_pct: 70,
            background_minute_pct: 65,
        }));
        let t0 = 1_700_000_000_000i64;
        let mut handles = Vec::new();
        for _ in 0..12 {
            let qc = q.clone();
            handles.push(std::thread::spawn(move || {
                qc.try_reserve(QuotaPriority::Interactive, 50, t0).is_ok()
            }));
        }
        let ok_count = handles.into_iter().map(|h| h.join().unwrap()).filter(|ok| *ok).count();
        assert_eq!(ok_count, 5);
        assert_eq!(q.snapshot(t0).day_requests, 5);
    }
}
