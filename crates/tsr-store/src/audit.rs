//! Content-free audit events (plan §15): the event name is a fixed string and the detail holds
//! only named numbers, so no content can reach the audit log through this path.

use rusqlite::{Connection, params};

use crate::StoreError;

/// Record `event` with numeric details, e.g. `("chars", 412)`.
pub fn record(
    conn: &Connection,
    at_ms: i64,
    event: &'static str,
    numbers: &[(&'static str, i64)],
) -> Result<(), StoreError> {
    let detail = serde_json::Value::Object(
        numbers
            .iter()
            .map(|(k, v)| ((*k).to_string(), serde_json::Value::from(*v)))
            .collect(),
    );
    conn.execute(
        "INSERT INTO audit_log (at, event, detail) VALUES (?1, ?2, ?3)",
        params![at_ms, event, detail.to_string()],
    )?;
    Ok(())
}

/// How many `event`s since `since_ms`.
pub fn count(conn: &Connection, event: &str, since_ms: i64) -> Result<u64, StoreError> {
    Ok(conn.query_row(
        "SELECT count(*) FROM audit_log WHERE event = ?1 AND at >= ?2",
        params![event, since_ms],
        |r| r.get::<_, i64>(0),
    )? as u64)
}
