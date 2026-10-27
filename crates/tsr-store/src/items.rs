//! Sources, items and keyword search (plan §8.1, §9.2, §10.3 step 1, §15.5).
//!
//! Text going in must already be redacted (tsr-guard). Text coming out is wrapped in
//! [`Sensitive`] so it cannot be logged by accident.

use rusqlite::{Connection, params};
use tsr_core::Sensitive;

use crate::StoreError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceKind {
    Url,
    File,
    App,
}

impl SourceKind {
    fn as_str(self) -> &'static str {
        match self {
            SourceKind::Url => "url",
            SourceKind::File => "file",
            SourceKind::App => "app",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Acquisition {
    Uia,
    Ocr,
    Vlm,
    Ext,
}

impl Acquisition {
    fn as_str(self) -> &'static str {
        match self {
            Acquisition::Uia => "uia",
            Acquisition::Ocr => "ocr",
            Acquisition::Vlm => "vlm",
            Acquisition::Ext => "ext",
        }
    }
}

pub struct NewSource<'a> {
    pub kind: SourceKind,
    pub app_exe: &'a str,
    pub app_name: Option<&'a str>,
    /// Already redacted.
    pub window_title: Option<&'a str>,
    /// Cleaned with [`clean_url`] on insert.
    pub url: Option<&'a str>,
    pub file_path: Option<&'a str>,
    pub seen_at: i64,
}

/// Query parameters that commonly carry credentials or one-time values.
const SECRET_PARAMS: &[&str] = &[
    "token",
    "access_token",
    "id_token",
    "refresh_token",
    "auth",
    "authorization",
    "code",
    "state",
    "nonce",
    "sig",
    "signature",
    "key",
    "apikey",
    "api_key",
    "secret",
    "client_secret",
    "password",
    "pwd",
    "pass",
    "otp",
    "session",
    "sessionid",
    "session_id",
    "sid",
    "ticket",
    "jwt",
    "assertion",
    "samlresponse",
    "x-amz-signature",
    "x-amz-credential",
    "x-amz-security-token",
    "x-goog-signature",
    "x-goog-credential",
    "se",
    "sp",
    "sv",
];

/// Drop the fragment (OAuth implicit-flow tokens live there) and credential-like query
/// parameters; keep the rest so the source link still opens the right page.
pub fn clean_url(url: &str) -> String {
    let url = url.split('#').next().unwrap_or("");
    let Some((base, query)) = url.split_once('?') else {
        return url.to_owned();
    };
    let kept: Vec<&str> = query
        .split('&')
        .filter(|kv| {
            let name = kv.split('=').next().unwrap_or("").to_ascii_lowercase();
            !name.is_empty() && !SECRET_PARAMS.contains(&name.as_str())
        })
        .collect();
    if kept.is_empty() {
        base.to_owned()
    } else {
        format!("{base}?{}", kept.join("&"))
    }
}

/// Insert a source or refresh `last_seen` on the existing one. Returns its id.
pub fn upsert_source(conn: &Connection, s: &NewSource<'_>) -> Result<i64, StoreError> {
    let url = s.url.map(clean_url);
    Ok(conn.query_row(
        "INSERT INTO sources (kind, app_exe, app_name, window_title, url, file_path, first_seen, last_seen)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
         ON CONFLICT (kind, app_exe, IFNULL(url, ''), IFNULL(file_path, ''), IFNULL(window_title, ''))
         DO UPDATE SET last_seen = max(last_seen, excluded.last_seen),
                       app_name = COALESCE(excluded.app_name, app_name)
         RETURNING id",
        params![
            s.kind.as_str(),
            s.app_exe,
            s.app_name,
            s.window_title,
            url,
            s.file_path,
            s.seen_at
        ],
        |r| r.get(0),
    )?)
}

pub struct NewItem<'a> {
    pub source_id: i64,
    pub started_at: i64,
    pub ended_at: i64,
    pub acquisition: Acquisition,
    /// Already redacted.
    pub text: &'a Sensitive<String>,
    pub simhash: u64,
    pub lang: Option<&'a str>,
    pub redaction_count: u32,
}

pub fn insert_item(conn: &Connection, it: &NewItem<'_>) -> Result<i64, StoreError> {
    conn.execute(
        "INSERT INTO items (source_id, started_at, ended_at, acquisition, text, simhash, lang, redaction_count)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            it.source_id,
            it.started_at,
            it.ended_at,
            it.acquisition.as_str(),
            it.text.expose(),
            it.simhash as i64,
            it.lang,
            it.redaction_count
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Turn free text into a safe FTS5 query: every word becomes a quoted phrase, ANDed.
/// FTS5 operators, column filters and quotes in the input are therefore just text.
pub fn fts_query(user: &str) -> Option<String> {
    let terms: Vec<String> = user
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|t| !t.is_empty())
        .take(32)
        .map(|t| format!("\"{t}\""))
        .collect();
    (!terms.is_empty()).then(|| terms.join(" "))
}

#[derive(Debug)]
pub struct Hit {
    pub item_id: i64,
    pub source_id: i64,
    pub started_at: i64,
    /// Lower is better (FTS5 bm25).
    pub score: f64,
    pub snippet: Sensitive<String>,
}

/// Keyword search over item text and summaries, best matches first.
pub fn search(conn: &Connection, query: &str, limit: u32) -> Result<Vec<Hit>, StoreError> {
    let Some(q) = fts_query(query) else {
        return Ok(Vec::new());
    };
    let mut st = conn.prepare(
        "SELECT items.id, items.source_id, items.started_at, bm25(items_fts) AS score,
                snippet(items_fts, 0, '[', ']', '…', 16)
         FROM items_fts JOIN items ON items.id = items_fts.rowid
         WHERE items_fts MATCH ?1
         ORDER BY score LIMIT ?2",
    )?;
    let hits = st
        .query_map(params![q, limit.min(200)], |r| {
            Ok(Hit {
                item_id: r.get(0)?,
                source_id: r.get(1)?,
                started_at: r.get(2)?,
                score: r.get(3)?,
                snippet: Sensitive::new(r.get(4)?),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(hits)
}

/// Items whose `started_at` is in `[from_ms, to_ms)`. With `dry_run`, only count (plan §15.5:
/// the UI shows the count before deleting). Sources left without items are removed too.
pub fn delete_range(conn: &mut Connection, from_ms: i64, to_ms: i64, dry_run: bool) -> Result<u64, StoreError> {
    let tx = conn.transaction()?;
    let n: i64 = tx.query_row(
        "SELECT count(*) FROM items WHERE started_at >= ?1 AND started_at < ?2",
        params![from_ms, to_ms],
        |r| r.get(0),
    )?;
    if !dry_run {
        tx.execute(
            "DELETE FROM items WHERE started_at >= ?1 AND started_at < ?2",
            params![from_ms, to_ms],
        )?;
        tx.execute(
            "DELETE FROM sources WHERE id NOT IN (SELECT DISTINCT source_id FROM items)",
            [],
        )?;
        tx.commit()?;
    }
    Ok(n as u64)
}
