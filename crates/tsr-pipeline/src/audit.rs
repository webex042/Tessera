//! Re-validate what is *stored* (Stage 2 exit criterion "0 schema-invalid rows"): every fact row
//! against the schema rules, and every evidence row against the item text it points into.

use rusqlite::Connection;
use tsr_store::StoreError;
use tsr_store::facts;
use tsr_store::items::get_item;

use crate::normalize::char_slice;
use crate::schema::{CONFIDENCE, KINDS};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Problem {
    pub fact_id: i64,
    pub what: &'static str,
}

pub fn validate_stored(conn: &Connection) -> Result<(usize, Vec<Problem>), StoreError> {
    let rows = facts::all(conn)?;
    let mut problems = Vec::new();
    let mut p = |id, what| problems.push(Problem { fact_id: id, what });
    for f in &rows {
        if !KINDS.contains(&f.kind.as_str()) {
            p(f.id, "kind");
        }
        if !CONFIDENCE.contains(&f.confidence.as_str()) {
            p(f.id, "confidence");
        }
        let n = f.text.expose().chars().count();
        if n == 0 || n > 200 {
            p(f.id, "text_length");
        }
        if !tsr_store::facts::STATUSES.contains(&f.status.as_str()) {
            p(f.id, "status");
        }
        if f.due_at.is_some() != f.due_precision.is_some() {
            p(f.id, "due_pair");
        }
        if f.prompt_ver.is_empty() || f.model.is_empty() || f.redact_ver.is_empty() {
            p(f.id, "provenance");
        }
        let ev = facts::evidence(conn, f.id)?;
        if ev.is_empty() {
            p(f.id, "no_evidence");
        }
        for e in ev {
            let Some(item) = get_item(conn, e.item_id)? else {
                p(f.id, "evidence_item_missing");
                continue;
            };
            let span = char_slice(item.text.expose(), e.start_ch as usize, e.end_ch as usize);
            let matches = match e.matched.as_str() {
                "exact" => span == *e.quote.expose(),
                // Normalised and fuzzy matches keep the source span; the quote must be that span.
                _ => !span.is_empty() && span == *e.quote.expose(),
            };
            if !matches {
                p(f.id, "evidence_span_mismatch");
            }
        }
    }
    Ok((rows.len(), problems))
}
