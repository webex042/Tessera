//! Facts and their evidence (plan §8.3, §9.2).
//!
//! The pipeline validates everything before calling these functions; the table's CHECK
//! constraints are the second line of defence. A fact is always inserted together with at least
//! one evidence row in the caller's transaction, and a trigger deletes a fact once its last
//! evidence row is gone. Evidence inserts are idempotent (primary key), so a retried job cannot
//! add the same quote twice or inflate `merged_count`.

use rusqlite::{Connection, OptionalExtension, params};
use tsr_core::Sensitive;

use crate::StoreError;

pub const KINDS: [&str; 6] = ["task", "commitment", "decision", "event", "question", "note"];
pub const STATUSES: [&str; 5] = ["open", "done", "dropped", "proposed_done", "needs_review"];
pub const CONFIDENCE: [&str; 3] = ["low", "medium", "high"];
pub const PRECISIONS: [&str; 4] = ["datetime", "date", "week", "month"];
pub const MATCHES: [&str; 3] = ["exact", "normalized", "fuzzy"];

#[derive(Debug)]
pub struct NewFact<'a> {
    pub kind: &'a str,
    pub text: &'a str,
    pub text_norm: &'a str,
    pub owner_id: Option<i64>,
    pub owner_text: Option<&'a str>,
    pub counterparty_id: Option<i64>,
    pub counterparty_text: Option<&'a str>,
    pub due_text: Option<&'a str>,
    /// (UTC ms, precision, ambiguous)
    pub due: Option<(i64, &'a str, bool)>,
    pub status: &'a str,
    pub confidence: &'a str,
    pub prompt_ver: &'a str,
    pub model: &'a str,
    pub schema_ver: u32,
    pub redactions: u32,
    pub redact_ver: &'a str,
    pub seen_at: i64,
}

#[derive(Debug, Clone)]
pub struct NewEvidence<'a> {
    pub item_id: i64,
    pub quote: &'a str,
    pub start_ch: i64,
    pub end_ch: i64,
    pub matched: &'a str,
}

/// Insert a fact with its evidence. Refuses a fact without evidence.
pub fn insert_fact(conn: &Connection, f: &NewFact<'_>, ev: &[NewEvidence<'_>]) -> Result<i64, StoreError> {
    if ev.is_empty() {
        return Err(StoreError::Invalid("fact without evidence"));
    }
    let (due_at, prec, amb) = match f.due {
        Some((a, p, amb)) => (Some(a), Some(p), amb),
        None => (None, None, false),
    };
    conn.execute(
        "INSERT INTO facts (kind, text, text_norm, owner_id, owner_text, counterparty_id, counterparty_text,
                            due_text, due_at, due_precision, due_ambiguous, status, confidence, prompt_ver,
                            model, schema_ver, redactions, redact_ver, first_seen, last_seen)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?19)",
        params![
            f.kind,
            f.text,
            f.text_norm,
            f.owner_id,
            f.owner_text,
            f.counterparty_id,
            f.counterparty_text,
            f.due_text,
            due_at,
            prec,
            amb,
            f.status,
            f.confidence,
            f.prompt_ver,
            f.model,
            f.schema_ver,
            f.redactions,
            f.redact_ver,
            f.seen_at
        ],
    )?;
    let id = conn.last_insert_rowid();
    for e in ev {
        add_evidence(conn, id, e, f.seen_at)?;
    }
    Ok(id)
}

/// Returns whether the row was new. Only a duplicate key is ignored: `INSERT OR IGNORE` would
/// also swallow CHECK failures (a bad span) and leave a fact without evidence.
pub fn add_evidence(conn: &Connection, fact_id: i64, e: &NewEvidence<'_>, now: i64) -> Result<bool, StoreError> {
    Ok(conn.execute(
        "INSERT INTO fact_evidence (fact_id, item_id, quote, start_ch, end_ch, match, added_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT (fact_id, item_id, start_ch) DO NOTHING",
        params![fact_id, e.item_id, e.quote, e.start_ch, e.end_ch, e.matched, now],
    )? == 1)
}

/// Merge a new sighting into an existing fact: add evidence; if it was new, advance `last_seen`
/// and count the merge. Fill a missing due date, never overwrite one. Idempotent.
pub fn merge_into(
    conn: &Connection,
    fact_id: i64,
    ev: &[NewEvidence<'_>],
    due: Option<(Option<&str>, i64, &str, bool)>,
    seen_at: i64,
) -> Result<bool, StoreError> {
    let mut added = false;
    for e in ev {
        added |= add_evidence(conn, fact_id, e, seen_at)?;
    }
    if added {
        conn.execute(
            "UPDATE facts SET last_seen = max(last_seen, ?2), merged_count = merged_count + 1 WHERE id = ?1",
            params![fact_id, seen_at],
        )?;
        if let Some((text, at, p, amb)) = due {
            conn.execute(
                "UPDATE facts SET due_text = COALESCE(due_text, ?2), due_at = ?3, due_precision = ?4, due_ambiguous = ?5
                 WHERE id = ?1 AND due_at IS NULL",
                params![fact_id, text, at, p, amb],
            )?;
        }
    }
    Ok(added)
}

#[derive(Debug, Clone)]
pub struct Candidate {
    pub id: i64,
    pub text_norm: Sensitive<String>,
    pub owner_text: Option<Sensitive<String>>,
    pub counterparty_id: Option<i64>,
    pub due_at: Option<i64>,
}

/// Open facts of this kind and owner seen since `since_ms` (plan §8.3 step 6: last 14 days).
pub fn dedupe_candidates(
    conn: &Connection,
    kind: &str,
    owner_id: Option<i64>,
    since_ms: i64,
) -> Result<Vec<Candidate>, StoreError> {
    let mut st = conn.prepare(
        "SELECT id, text_norm, owner_text, counterparty_id, due_at FROM facts
         WHERE kind = ?1 AND owner_id IS ?2 AND last_seen >= ?3 AND status IN ('open', 'needs_review')
         ORDER BY id",
    )?;
    let rows = st
        .query_map(params![kind, owner_id, since_ms], |r| {
            Ok(Candidate {
                id: r.get(0)?,
                text_norm: Sensitive::new(r.get(1)?),
                owner_text: r.get::<_, Option<String>>(2)?.map(Sensitive::new),
                counterparty_id: r.get(3)?,
                due_at: r.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// A stored fact as read back (for the UI, evals and the schema audit).
#[derive(Debug, Clone)]
pub struct FactRow {
    pub id: i64,
    pub kind: String,
    pub text: Sensitive<String>,
    pub owner_id: Option<i64>,
    pub owner_text: Option<Sensitive<String>>,
    pub counterparty_id: Option<i64>,
    pub counterparty_text: Option<Sensitive<String>>,
    pub due_text: Option<Sensitive<String>>,
    pub due_at: Option<i64>,
    pub due_precision: Option<String>,
    pub due_ambiguous: bool,
    pub status: String,
    pub confidence: String,
    pub prompt_ver: String,
    pub model: String,
    pub schema_ver: u32,
    pub redactions: u32,
    pub redact_ver: String,
    pub first_seen: i64,
    pub last_seen: i64,
    pub merged_count: u32,
}

const FACT_COLS: &str = "id, kind, text, owner_id, owner_text, counterparty_id, counterparty_text, due_text, due_at,
     due_precision, due_ambiguous, status, confidence, prompt_ver, model, schema_ver, redactions, redact_ver,
     first_seen, last_seen, merged_count";

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<FactRow> {
    let s = |i: usize| -> rusqlite::Result<Option<Sensitive<String>>> {
        Ok(r.get::<_, Option<String>>(i)?.map(Sensitive::new))
    };
    Ok(FactRow {
        id: r.get(0)?,
        kind: r.get(1)?,
        text: Sensitive::new(r.get(2)?),
        owner_id: r.get(3)?,
        owner_text: s(4)?,
        counterparty_id: r.get(5)?,
        counterparty_text: s(6)?,
        due_text: s(7)?,
        due_at: r.get(8)?,
        due_precision: r.get(9)?,
        due_ambiguous: r.get(10)?,
        status: r.get(11)?,
        confidence: r.get(12)?,
        prompt_ver: r.get(13)?,
        model: r.get(14)?,
        schema_ver: r.get(15)?,
        redactions: r.get(16)?,
        redact_ver: r.get(17)?,
        first_seen: r.get(18)?,
        last_seen: r.get(19)?,
        merged_count: r.get(20)?,
    })
}

pub fn get(conn: &Connection, id: i64) -> Result<Option<FactRow>, StoreError> {
    Ok(conn
        .query_row(&format!("SELECT {FACT_COLS} FROM facts WHERE id = ?1"), [id], row)
        .optional()?)
}

pub fn all(conn: &Connection) -> Result<Vec<FactRow>, StoreError> {
    let mut st = conn.prepare(&format!("SELECT {FACT_COLS} FROM facts ORDER BY id"))?;
    let rows = st.query_map([], row)?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Facts with evidence in this item.
pub fn for_item(conn: &Connection, item_id: i64) -> Result<Vec<FactRow>, StoreError> {
    let mut st = conn.prepare(&format!(
        "SELECT {FACT_COLS} FROM facts WHERE id IN (SELECT fact_id FROM fact_evidence WHERE item_id = ?1) ORDER BY id"
    ))?;
    let rows = st.query_map([item_id], row)?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

#[derive(Debug, Clone)]
pub struct EvidenceRow {
    pub item_id: i64,
    pub quote: Sensitive<String>,
    pub start_ch: i64,
    pub end_ch: i64,
    pub matched: String,
}

pub fn evidence(conn: &Connection, fact_id: i64) -> Result<Vec<EvidenceRow>, StoreError> {
    let mut st = conn.prepare(
        "SELECT item_id, quote, start_ch, end_ch, match FROM fact_evidence WHERE fact_id = ?1 ORDER BY item_id, start_ch",
    )?;
    let rows = st
        .query_map([fact_id], |r| {
            Ok(EvidenceRow {
                item_id: r.get(0)?,
                quote: Sensitive::new(r.get(1)?),
                start_ch: r.get(2)?,
                end_ch: r.get(3)?,
                matched: r.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn set_status(conn: &Connection, id: i64, status: &str) -> Result<bool, StoreError> {
    if !STATUSES.contains(&status) {
        return Err(StoreError::Invalid("fact status"));
    }
    Ok(conn.execute("UPDATE facts SET status = ?2 WHERE id = ?1", params![id, status])? == 1)
}

/// Keyword search over fact text (hybrid search's fact channel).
pub fn search(conn: &Connection, query: &str, limit: u32) -> Result<Vec<(i64, f64)>, StoreError> {
    let Some(q) = crate::items::fts_query(query) else {
        return Ok(Vec::new());
    };
    let mut st = conn.prepare(
        "SELECT rowid, bm25(facts_fts) AS s FROM facts_fts WHERE facts_fts MATCH ?1 ORDER BY s, rowid LIMIT ?2",
    )?;
    let rows = st
        .query_map(params![q, limit.min(200)], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}
