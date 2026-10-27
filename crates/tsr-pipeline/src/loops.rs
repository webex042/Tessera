//! Open loops (plan §11.3).
//!
//! - An open loop is a fact with `status = open` and kind task, commitment or question.
//! - **Nothing closes silently.** When later extraction finds completion evidence for an open
//!   task or commitment ("sent the deck to Lena"), the loop becomes `proposed_done` with that
//!   evidence attached; only the user's confirmation (an approval token in the app) makes it
//!   `done`. This module can only ever write `proposed_done`.
//! - Loops with no new evidence for 21 days are grouped under "Still open?".

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;
use rusqlite::{Connection, params};
use tsr_store::StoreError;
use tsr_store::facts::{self, FactRow, NewEvidence};
use tsr_store::people;

use crate::normalize::normalize;

pub const LOOP_KINDS: [&str; 3] = ["task", "commitment", "question"];
pub const STALE_AFTER_MS: i64 = 21 * 86_400_000;
/// Completion evidence is matched against loops seen in the last 60 days.
pub const MATCH_WINDOW_MS: i64 = 60 * 86_400_000;
pub const MIN_SIMILARITY: f32 = 0.5;

/// Past-tense completion language ("sent", "done", "submitted", "replied", …).
static COMPLETION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(sent|shared|submitted|done|completed|finished|delivered|replied|answered|paid|uploaded|merged|closed|fixed|resolved|signed|booked|scheduled|emailed|forwarded|published|approved|returned|handed over|wrapped up)\b",
    )
    .expect("regex")
});

/// Text that reports something as finished (not a plan or a request to do it).
pub fn completion_signal(text: &str) -> bool {
    let n = normalize(text).text;
    static NOT_YET: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\b(not|never|haven't|hasn't|didn't|will|to be|should|please|can you|could you|needs? to)\b")
            .expect("regex")
    });
    COMPLETION.is_match(&n) && !NOT_YET.is_match(&n)
}

const STOP: [&str; 34] = [
    "the", "a", "an", "to", "of", "for", "and", "or", "in", "on", "at", "by", "with", "about", "from", "is", "be",
    "it", "this", "that", "i", "me", "my", "we", "our", "you", "your", "he", "she", "they", "them", "his", "her",
    "their",
];

/// Action verbs in their plan form; with the past forms caught by `COMPLETION`, they are left
/// out when comparing, so "send the deck to Lena" and "sent the deck to Lena" compare on
/// "deck lena".
const ACTIONS: [&str; 24] = [
    "send", "share", "submit", "finish", "complete", "reply", "answer", "pay", "upload", "merge", "close", "fix",
    "resolve", "sign", "book", "schedule", "email", "forward", "publish", "approve", "return", "deliver", "do", "wrap",
];

/// Content words: normalised, without stop words and action verbs.
fn content_words(s: &str) -> HashSet<String> {
    let n = normalize(s).text;
    n.split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 1 && !STOP.contains(w) && !ACTIONS.contains(w) && !COMPLETION.is_match(w))
        .map(str::to_string)
        .collect()
}

fn dice(a: &HashSet<String>, b: &HashSet<String>) -> f32 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    2.0 * a.intersection(b).count() as f32 / (a.len() + b.len()) as f32
}

/// A just-extracted fact that may report a loop as finished.
pub struct Completion<'a> {
    pub text: &'a str,
    pub counterparty_id: Option<i64>,
    /// The whole (redacted) item the completion came from.
    pub item_text: &'a str,
    pub evidence: NewEvidence<'a>,
}

/// Whether `text` mentions person `id` by a name or alias (first name counts).
fn mentions(conn: &Connection, id: i64, text: &str) -> Result<bool, StoreError> {
    let words: HashSet<String> = normalize(text)
        .text
        .split(|c: char| !c.is_alphanumeric() && c != '@' && c != '.')
        .map(str::to_string)
        .collect();
    let mut names: Vec<String> = people::aliases_of(conn, id)?
        .into_iter()
        .map(|(_, a)| a.expose().clone())
        .collect();
    if let Some(p) = people::person(conn, id)? {
        names.push(p.display_name.expose().clone());
    }
    Ok(names.iter().any(|n| {
        let n = normalize(n).text;
        words.contains(&n)
            || n.split_whitespace()
                .next()
                .is_some_and(|first| first.len() >= 3 && words.contains(first))
    }))
}

/// Mark open tasks and commitments that `c` reports as finished `proposed_done`, attaching the
/// completion evidence. Returns the fact ids changed. Never writes `done`.
pub fn propose_completions(conn: &Connection, c: &Completion<'_>, now: i64) -> Result<Vec<i64>, StoreError> {
    if !completion_signal(c.text) {
        return Ok(Vec::new());
    }
    let words = content_words(c.text);
    let mut best: Option<(f32, i64)> = None;
    let mut st = conn.prepare(
        "SELECT id, text, counterparty_id FROM facts
          WHERE status = 'open' AND kind IN ('task', 'commitment') AND last_seen >= ?1 AND last_seen <= ?2",
    )?;
    let rows = st.query_map(params![now - MATCH_WINDOW_MS, now], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<i64>>(2)?))
    })?;
    for r in rows {
        let (id, text, cp) = r?;
        // The loop's other person must be the completion's: the same resolved person, or, if the
        // completion names nobody, mentioned in its item ("Hi Ravi, sent the sheet").
        match (cp, c.counterparty_id) {
            (Some(a), Some(b)) if a != b => continue,
            (Some(a), None) if !mentions(conn, a, c.item_text)? => continue,
            _ => {}
        }
        let s = dice(&words, &content_words(&text));
        if s >= MIN_SIMILARITY && best.is_none_or(|(bs, bid)| s > bs || (s == bs && id < bid)) {
            best = Some((s, id));
        }
    }
    let Some((_, id)) = best else { return Ok(Vec::new()) };
    let changed = conn.execute(
        "UPDATE facts SET status = 'proposed_done', last_seen = MAX(last_seen, ?2) WHERE id = ?1 AND status = 'open'",
        params![id, now],
    )?;
    if changed == 0 {
        return Ok(Vec::new());
    }
    facts::add_evidence(conn, id, &c.evidence, now)?;
    Ok(vec![id])
}

/// Open loops in three groups for the UI.
#[derive(Debug, Default)]
pub struct Groups {
    /// Proposed as done by later evidence; waiting for the user's confirmation.
    pub proposed_done: Vec<FactRow>,
    pub open: Vec<FactRow>,
    /// No new evidence for 21 days: "Still open?".
    pub still_open: Vec<FactRow>,
}

pub fn groups(conn: &Connection, now: i64) -> Result<Groups, StoreError> {
    let mut g = Groups::default();
    for f in facts::all(conn)? {
        if !LOOP_KINDS.contains(&f.kind.as_str()) {
            continue;
        }
        match f.status.as_str() {
            "proposed_done" => g.proposed_done.push(f),
            "open" if f.last_seen < now - STALE_AFTER_MS => g.still_open.push(f),
            "open" => g.open.push(f),
            _ => {}
        }
    }
    let key = |f: &FactRow| (f.due_at.unwrap_or(i64::MAX), f.first_seen, f.id);
    g.open.sort_by_key(key);
    g.proposed_done.sort_by_key(key);
    g.still_open.sort_by_key(key);
    Ok(g)
}
