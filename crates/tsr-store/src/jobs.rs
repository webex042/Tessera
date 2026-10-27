//! Durable job state (plan §8.1). The in-memory scheduler decides order; this table remembers
//! what still has to happen across restarts. One row per (kind, item); states only move forward
//! except `running → queued` after an interruption.

use rusqlite::{Connection, OptionalExtension, params};

use crate::StoreError;

pub const KINDS: [&str; 3] = ["extract", "embed", "vlm"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobState {
    Queued,
    Running,
    Done,
    Failed,
    Skipped,
    Cancelled,
}

impl JobState {
    pub fn as_str(self) -> &'static str {
        match self {
            JobState::Queued => "queued",
            JobState::Running => "running",
            JobState::Done => "done",
            JobState::Failed => "failed",
            JobState::Skipped => "skipped",
            JobState::Cancelled => "cancelled",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "queued" => JobState::Queued,
            "running" => JobState::Running,
            "done" => JobState::Done,
            "failed" => JobState::Failed,
            "skipped" => JobState::Skipped,
            "cancelled" => JobState::Cancelled,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobRow {
    pub id: i64,
    pub kind: String,
    pub item_id: i64,
    pub state: JobState,
    pub attempts: u32,
    pub stage: Option<String>,
    pub error: Option<String>,
}

/// Create the job if it does not exist. Returns its id and whether it was created.
pub fn enqueue(conn: &Connection, kind: &str, item_id: i64, now: i64) -> Result<(i64, bool), StoreError> {
    if !KINDS.contains(&kind) {
        return Err(StoreError::Invalid("job kind"));
    }
    let n = conn.execute(
        "INSERT INTO jobs (kind, item_id, state, created_at, updated_at) VALUES (?1, ?2, 'queued', ?3, ?3) ON CONFLICT DO NOTHING",
        params![kind, item_id, now],
    )?;
    let id = conn.query_row(
        "SELECT id FROM jobs WHERE kind = ?1 AND item_id = ?2",
        params![kind, item_id],
        |r| r.get(0),
    )?;
    Ok((id, n == 1))
}

pub fn get(conn: &Connection, kind: &str, item_id: i64) -> Result<Option<JobRow>, StoreError> {
    Ok(conn
        .query_row(
            "SELECT id, kind, item_id, state, attempts, stage, error FROM jobs WHERE kind = ?1 AND item_id = ?2",
            params![kind, item_id],
            |r| {
                let s: String = r.get(3)?;
                Ok(JobRow {
                    id: r.get(0)?,
                    kind: r.get(1)?,
                    item_id: r.get(2)?,
                    state: JobState::parse(&s).unwrap_or(JobState::Failed),
                    attempts: r.get(4)?,
                    stage: r.get(5)?,
                    error: r.get(6)?,
                })
            },
        )
        .optional()?)
}

/// Mark running and count the attempt. Refuses jobs that are already finished.
pub fn start(conn: &Connection, id: i64, now: i64) -> Result<bool, StoreError> {
    Ok(conn.execute(
        "UPDATE jobs SET state = 'running', attempts = attempts + 1, updated_at = ?2, error = NULL
         WHERE id = ?1 AND state IN ('queued', 'running')",
        params![id, now],
    )? == 1)
}

pub fn set_stage(conn: &Connection, id: i64, stage: &'static str, now: i64) -> Result<(), StoreError> {
    conn.execute(
        "UPDATE jobs SET stage = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, stage, now],
    )?;
    Ok(())
}

/// Final or retry state. `error` must be a content-free code.
pub fn finish(conn: &Connection, id: i64, state: JobState, error: Option<&str>, now: i64) -> Result<(), StoreError> {
    if let Some(e) = error
        && (e.len() > 64
            || !e
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b == b'_' || b.is_ascii_digit()))
    {
        return Err(StoreError::Invalid("job error must be a short code"));
    }
    conn.execute(
        "UPDATE jobs SET state = ?2, error = ?3, updated_at = ?4 WHERE id = ?1",
        params![id, state.as_str(), error, now],
    )?;
    Ok(())
}

/// After a crash or shutdown: anything left `running` goes back to `queued`.
pub fn requeue_interrupted(conn: &Connection, now: i64) -> Result<u64, StoreError> {
    Ok(conn.execute(
        "UPDATE jobs SET state = 'queued', updated_at = ?1 WHERE state = 'running'",
        [now],
    )? as u64)
}

/// Queued jobs of a kind, oldest first.
pub fn queued(conn: &Connection, kind: &str, limit: u32) -> Result<Vec<(i64, i64)>, StoreError> {
    let mut st = conn.prepare(
        "SELECT id, item_id FROM jobs WHERE kind = ?1 AND state = 'queued' ORDER BY created_at, id LIMIT ?2",
    )?;
    let rows = st
        .query_map(params![kind, limit], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Content-free backlog counts per (kind, state) for the diagnostics page.
pub fn counts(conn: &Connection) -> Result<Vec<(String, String, u64)>, StoreError> {
    let mut st = conn.prepare("SELECT kind, state, count(*) FROM jobs GROUP BY kind, state ORDER BY kind, state")?;
    let rows = st
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get::<_, i64>(2)? as u64)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}
