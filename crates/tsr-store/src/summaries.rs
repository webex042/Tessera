//! Rollup summaries with provenance, and morning briefs (plan §9.2, §11).
//!
//! A summary is replaced as a whole (row, highlights and sources in one transaction), so a reader
//! never sees a summary whose highlights point at sources it does not list. Text is wrapped in
//! [`Sensitive`]: it is derived from screen content.

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use tsr_core::Sensitive;

use crate::StoreError;

pub const LEVELS: [&str; 4] = ["hour", "day", "week", "month"];

/// One highlight. `item_ids` always lists the original items it rests on (for day and up, the
/// union of the cited children's items, computed by code); `child_ids` the child summaries cited.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Highlight {
    pub text: String,
    pub item_ids: Vec<i64>,
    #[serde(default)]
    pub child_ids: Vec<i64>,
}

impl zeroize::Zeroize for Highlight {
    fn zeroize(&mut self) {
        self.text.zeroize();
        self.item_ids.zeroize();
        self.child_ids.zeroize();
    }
}

pub struct NewSummary<'a> {
    pub level: &'a str,
    pub period_start: i64,
    pub period_end: i64,
    pub text: &'a str,
    pub highlights: &'a [Highlight],
    /// Provenance: items (hour level) and child summaries (day and up).
    pub item_ids: &'a [i64],
    pub child_ids: &'a [i64],
    pub prompt_ver: &'a str,
    pub model: &'a str,
    pub now: i64,
}

#[derive(Debug, Clone)]
pub struct SummaryRow {
    pub id: i64,
    pub level: String,
    pub period_start: i64,
    pub period_end: i64,
    pub text: Sensitive<String>,
    pub highlights: Sensitive<Vec<Highlight>>,
    pub stale: bool,
    pub prompt_ver: String,
    pub model: String,
    pub created_at: i64,
}

const COLS: &str = "id, level, period_start, period_end, text, highlights, stale, prompt_ver, model, created_at";

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<SummaryRow> {
    let h: String = r.get(5)?;
    Ok(SummaryRow {
        id: r.get(0)?,
        level: r.get(1)?,
        period_start: r.get(2)?,
        period_end: r.get(3)?,
        text: Sensitive::new(r.get(4)?),
        // The column is CHECKed json_valid; a shape mismatch reads as no highlights.
        highlights: Sensitive::new(serde_json::from_str(&h).unwrap_or_default()),
        stale: r.get(6)?,
        prompt_ver: r.get(7)?,
        model: r.get(8)?,
        created_at: r.get(9)?,
    })
}

/// Insert or replace the summary for `(level, period_start)` with its sources. Returns its id.
pub fn upsert(conn: &mut Connection, s: &NewSummary<'_>) -> Result<i64, StoreError> {
    if !LEVELS.contains(&s.level) {
        return Err(StoreError::Invalid("summary level"));
    }
    let highlights = serde_json::to_string(s.highlights).map_err(|_| StoreError::Invalid("highlights"))?;
    let tx = conn.transaction()?;
    tx.execute(
        "DELETE FROM summaries WHERE level = ?1 AND period_start = ?2",
        params![s.level, s.period_start],
    )?;
    tx.execute(
        "INSERT INTO summaries(level, period_start, period_end, text, highlights, stale, prompt_ver, model, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, ?7, ?8)",
        params![
            s.level,
            s.period_start,
            s.period_end,
            s.text,
            highlights,
            s.prompt_ver,
            s.model,
            s.now
        ],
    )?;
    let id = tx.last_insert_rowid();
    {
        let mut ins_item = tx.prepare("INSERT INTO summary_sources(summary_id, item_id) VALUES (?1, ?2)")?;
        for i in s.item_ids {
            ins_item.execute(params![id, i])?;
        }
        let mut ins_child = tx.prepare("INSERT INTO summary_sources(summary_id, child_id) VALUES (?1, ?2)")?;
        for c in s.child_ids {
            ins_child.execute(params![id, c])?;
        }
    }
    tx.commit()?;
    Ok(id)
}

pub fn get(conn: &Connection, level: &str, period_start: i64) -> Result<Option<SummaryRow>, StoreError> {
    Ok(conn
        .query_row(
            &format!("SELECT {COLS} FROM summaries WHERE level = ?1 AND period_start = ?2"),
            params![level, period_start],
            row,
        )
        .optional()?)
}

pub fn by_id(conn: &Connection, id: i64) -> Result<Option<SummaryRow>, StoreError> {
    Ok(conn
        .query_row(&format!("SELECT {COLS} FROM summaries WHERE id = ?1"), [id], row)
        .optional()?)
}

/// Summaries of `level` that start in `[start, end)`, oldest first.
pub fn in_range(conn: &Connection, level: &str, start: i64, end: i64) -> Result<Vec<SummaryRow>, StoreError> {
    let mut st = conn.prepare(&format!(
        "SELECT {COLS} FROM summaries WHERE level = ?1 AND period_start >= ?2 AND period_start < ?3
         ORDER BY period_start"
    ))?;
    Ok(st
        .query_map(params![level, start, end], row)?
        .collect::<Result<_, _>>()?)
}

/// Stale summaries to regenerate, finest level first (a day needs its hours first).
pub fn stale(conn: &Connection, limit: u32) -> Result<Vec<SummaryRow>, StoreError> {
    let mut st = conn.prepare(&format!(
        "SELECT {COLS} FROM summaries WHERE stale = 1
         ORDER BY CASE level WHEN 'hour' THEN 0 WHEN 'day' THEN 1 WHEN 'week' THEN 2 ELSE 3 END, period_start
         LIMIT ?1"
    ))?;
    Ok(st.query_map([limit], row)?.collect::<Result<_, _>>()?)
}

/// Every item a summary rests on, through any depth of child summaries (drill-down).
pub fn source_items(conn: &Connection, summary_id: i64) -> Result<Vec<i64>, StoreError> {
    let mut st = conn.prepare(
        "WITH RECURSIVE tree(id) AS (
             SELECT ?1
             UNION SELECT s.child_id FROM summary_sources s JOIN tree ON s.summary_id = tree.id
              WHERE s.child_id IS NOT NULL)
         SELECT DISTINCT s.item_id FROM summary_sources s JOIN tree ON s.summary_id = tree.id
          WHERE s.item_id IS NOT NULL
          ORDER BY s.item_id",
    )?;
    Ok(st.query_map([summary_id], |r| r.get(0))?.collect::<Result<_, _>>()?)
}

/// Direct child summaries of a summary.
pub fn children(conn: &Connection, summary_id: i64) -> Result<Vec<i64>, StoreError> {
    let mut st = conn.prepare(
        "SELECT child_id FROM summary_sources WHERE summary_id = ?1 AND child_id IS NOT NULL ORDER BY child_id",
    )?;
    Ok(st.query_map([summary_id], |r| r.get(0))?.collect::<Result<_, _>>()?)
}

pub fn put_brief(conn: &Connection, day: &str, json: &str, now: i64) -> Result<(), StoreError> {
    conn.execute(
        "INSERT INTO briefs(day, json, created_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(day) DO UPDATE SET json = excluded.json, created_at = excluded.created_at",
        params![day, json, now],
    )?;
    Ok(())
}

pub fn get_brief(conn: &Connection, day: &str) -> Result<Option<(Sensitive<String>, i64)>, StoreError> {
    Ok(conn
        .query_row("SELECT json, created_at FROM briefs WHERE day = ?1", [day], |r| {
            Ok((Sensitive::new(r.get::<_, String>(0)?), r.get(1)?))
        })
        .optional()?)
}
