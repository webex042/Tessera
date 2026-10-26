//! Blocklist rules and per-rule block counts (plan §6.1, §9.2).
//!
//! Blocks are recorded as `blocked_window` audit events whose detail holds only the rule key and
//! the reason, e.g. `{"rule":"builtin.title.otp","reason":"title"}`. Counting them per rule lets
//! the settings page show over-broad rules without ever storing a title or URL.

use rusqlite::{Connection, OptionalExtension, params};
use tsr_guard::blocklist::{Blocklist, Decision, RuleKind, RuleSpec, Scope};

use crate::StoreError;

pub const BLOCKED_WINDOW_EVENT: &str = "blocked_window";

fn row_to_spec(r: &rusqlite::Row<'_>) -> rusqlite::Result<RuleSpec> {
    let kind: String = r.get("kind")?;
    let scope: String = r.get("scope")?;
    Ok(RuleSpec {
        key: r.get("key")?,
        // CHECK constraints guarantee these parse.
        kind: RuleKind::parse(&kind).unwrap_or(RuleKind::Title),
        scope: Scope::parse(&scope).unwrap_or(Scope::All),
        pattern: r.get("pattern")?,
        is_regex: r.get("is_regex")?,
        builtin: r.get("builtin")?,
        enabled: r.get("enabled")?,
    })
}

/// Insert or update the shipped built-in rules. Pattern, kind and scope follow the new defaults;
/// the user's enabled/disabled choice is kept. Built-ins no longer shipped are removed.
pub fn sync_builtin_rules(conn: &mut Connection, builtins: &[RuleSpec]) -> Result<(), StoreError> {
    let tx = conn.transaction()?;
    for s in builtins {
        tx.execute(
            "INSERT INTO blocklist (key, kind, scope, pattern, is_regex, builtin, enabled)
             VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6)
             ON CONFLICT(key) DO UPDATE SET
               kind = excluded.kind, scope = excluded.scope,
               pattern = excluded.pattern, is_regex = excluded.is_regex, builtin = 1",
            params![
                s.key,
                s.kind.as_str(),
                s.scope.as_str(),
                s.pattern,
                s.is_regex,
                s.enabled
            ],
        )?;
    }
    let keys: Vec<&str> = builtins.iter().map(|s| s.key.as_str()).collect();
    let mut stale = Vec::new();
    {
        let mut st = tx.prepare("SELECT key FROM blocklist WHERE builtin = 1")?;
        for k in st.query_map([], |r| r.get::<_, String>(0))? {
            let k = k?;
            if !keys.contains(&k.as_str()) {
                stale.push(k);
            }
        }
    }
    for k in stale {
        tx.execute("DELETE FROM blocklist WHERE key = ?1", [k])?;
    }
    tx.commit()?;
    Ok(())
}

pub fn load_rules(conn: &Connection) -> Result<Vec<RuleSpec>, StoreError> {
    let mut st =
        conn.prepare("SELECT key, kind, scope, pattern, is_regex, builtin, enabled FROM blocklist ORDER BY id")?;
    let rules = st.query_map([], row_to_spec)?.collect::<Result<Vec<_>, _>>()?;
    Ok(rules)
}

/// Enable or disable any rule (built-in or user). Returns false if the key does not exist.
pub fn set_enabled(conn: &Connection, key: &str, enabled: bool) -> Result<bool, StoreError> {
    Ok(conn.execute(
        "UPDATE blocklist SET enabled = ?2 WHERE key = ?1",
        params![key, enabled],
    )? == 1)
}

/// Add a user rule. Its key is `user.<row id>`, so the key never contains the pattern (which may
/// itself be sensitive, e.g. a bank's name). Regexes are validated before anything is stored.
pub fn add_user_rule(
    conn: &mut Connection,
    kind: RuleKind,
    scope: Scope,
    pattern: &str,
    is_regex: bool,
) -> Result<String, StoreError> {
    if pattern.trim().is_empty() || pattern.len() > 500 {
        return Err(StoreError::InvalidRule);
    }
    let probe = RuleSpec {
        key: "user.new".into(),
        kind,
        scope,
        pattern: pattern.into(),
        is_regex,
        builtin: false,
        enabled: true,
    };
    if !Blocklist::compile(std::slice::from_ref(&probe)).1.is_empty() {
        return Err(StoreError::InvalidRule);
    }
    let tx = conn.transaction()?;
    // Unique placeholder key, replaced by the id-based key in the same transaction.
    tx.execute(
        "INSERT INTO blocklist (key, kind, scope, pattern, is_regex, builtin, enabled)
         VALUES ('user.pending.' || hex(randomblob(8)), ?1, ?2, ?3, ?4, 0, 1)",
        params![kind.as_str(), scope.as_str(), pattern, is_regex],
    )?;
    let id = tx.last_insert_rowid();
    let key = format!("user.{id}");
    tx.execute("UPDATE blocklist SET key = ?2 WHERE id = ?1", params![id, key])?;
    tx.commit()?;
    Ok(key)
}

/// Delete a user rule. Built-ins cannot be deleted, only disabled.
pub fn remove_user_rule(conn: &Connection, key: &str) -> Result<bool, StoreError> {
    Ok(conn.execute("DELETE FROM blocklist WHERE key = ?1 AND builtin = 0", [key])? == 1)
}

/// Record a block in the audit log. `Allow` decisions record nothing.
pub fn record_block(conn: &Connection, decision: &Decision, at_ms: i64) -> Result<(), StoreError> {
    let Decision::Block { reason, rule_key } = decision else {
        return Ok(());
    };
    let rule = rule_key.as_deref().unwrap_or("none");
    // Keys are generated by us (`builtin.*`, `user.<n>`); refuse anything else so content can
    // never reach the audit log through this path.
    if !rule
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'_')
    {
        return Err(StoreError::InvalidRule);
    }
    let detail = format!(r#"{{"rule":"{rule}","reason":"{}"}}"#, reason.as_str());
    conn.execute(
        "INSERT INTO audit_log (at, event, detail) VALUES (?1, ?2, ?3)",
        params![at_ms, BLOCKED_WINDOW_EVENT, detail],
    )?;
    Ok(())
}

/// How many windows each rule blocked since `since_ms`, most first. Rules with no blocks are
/// omitted.
pub fn blocked_counts(conn: &Connection, since_ms: i64) -> Result<Vec<(String, u64)>, StoreError> {
    let mut st = conn.prepare(
        "SELECT json_extract(detail, '$.rule') AS rule, count(*) AS n
         FROM audit_log WHERE event = ?1 AND at >= ?2
         GROUP BY rule ORDER BY n DESC, rule",
    )?;
    let rows = st
        .query_map(params![BLOCKED_WINDOW_EVENT, since_ms], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Look up one rule by key.
pub fn get_rule(conn: &Connection, key: &str) -> Result<Option<RuleSpec>, StoreError> {
    Ok(conn
        .query_row(
            "SELECT key, kind, scope, pattern, is_regex, builtin, enabled FROM blocklist WHERE key = ?1",
            [key],
            row_to_spec,
        )
        .optional()?)
}
