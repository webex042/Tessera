//! Content-free diagnostics bundle for beta testers (plan §17 Stage 9).
//!
//! Only counts, states, timings, versions and hardware facts. Never text, window titles, URLs,
//! people, app names or file paths. The UI shows the exact JSON; the tester saves it and
//! decides whether to send it. Tessara itself never sends anything.

use rusqlite::Connection;
use serde_json::{Value, json};

fn count(c: &Connection, sql: &str) -> i64 {
    c.query_row(sql, [], |r| r.get(0)).unwrap_or(-1)
}

fn grouped(c: &Connection, sql: &str) -> Value {
    let mut m = serde_json::Map::new();
    if let Ok(mut st) = c.prepare(sql)
        && let Ok(rows) = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
    {
        for (k, n) in rows.flatten() {
            // Keys come from CHECK-constrained enum columns; anything unexpected is not echoed.
            let safe = k.len() <= 40
                && k.bytes()
                    .all(|b| b.is_ascii_lowercase() || b == b'_' || b.is_ascii_digit());
            m.insert(if safe { k } else { "other".into() }, json!(n));
        }
    }
    Value::Object(m)
}

/// The bundle. `status` and `runtime` are already content-free structures.
pub fn bundle(
    c: &Connection,
    status: &crate::services::Status,
    runtime: Option<Value>,
    start_error: Option<&str>,
) -> Value {
    let schema: i64 = c.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap_or(-1);
    json!({
        "format": "tessara-diagnostics/1",
        "app": {
            "version": env!("CARGO_PKG_VERSION"),
            "build": if cfg!(debug_assertions) { "debug" } else { "release" },
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
        },
        "start_error": start_error,
        "capture": { "paused": status.paused, "capturing": status.capturing },
        "model": { "state": status.model.state, "backend": status.model.backend },
        "queue_depth": status.queue,
        "runtime": runtime,
        "store": {
            "schema_version": schema,
            "items": count(c, "SELECT count(*) FROM items"),
            "sources": count(c, "SELECT count(*) FROM sources"),
            "items_by_acquisition": grouped(c, "SELECT acquisition, count(*) FROM items GROUP BY acquisition"),
            "items_by_extraction": grouped(c, "SELECT extraction, count(*) FROM items GROUP BY extraction"),
            "facts_by_kind": grouped(c, "SELECT kind, count(*) FROM facts GROUP BY kind"),
            "facts_by_status": grouped(c, "SELECT status, count(*) FROM facts GROUP BY status"),
            "summaries_by_level": grouped(c, "SELECT level, count(*) FROM summaries GROUP BY level"),
            "stale_summaries": count(c, "SELECT count(*) FROM summaries WHERE stale = 1"),
            "summaries_extractive": count(c, "SELECT count(*) FROM summaries WHERE model = 'extractive'"),
            "jobs_by_state": grouped(c, "SELECT kind || '_' || state, count(*) FROM jobs GROUP BY kind, state"),
            "people": count(c, "SELECT count(*) FROM people"),
            "audit_events": grouped(c, "SELECT event, count(*) FROM audit_log GROUP BY event"),
        },
    })
}
