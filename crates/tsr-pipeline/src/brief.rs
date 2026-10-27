//! Morning brief (plan §11.2), assembled by code from the store.
//!
//! - **When:** the first unlock (or app start) at or after `brief_time` (default 05:00 local) on a
//!   day that has no brief yet ([`due_day`]).
//! - **What:** overdue and due-today loops; open commitments by the user and to the user;
//!   questions the user is waiting on; yesterday's day summary (already cited).
//! - **Opener:** written by code from the counts. The plan allows an optional model-written
//!   opener; it is not used, so the brief never waits for the model and contains no model text
//!   beyond yesterday's already-validated summary (ADR-0008).

use jiff::tz::TimeZone;
use jiff::{Timestamp, ToSpan, Zoned};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tsr_store::StoreError;
use tsr_store::facts::{self, FactRow};
use tsr_store::people;
use tsr_store::summaries::{self, Highlight};

use crate::rollup::{Level, period_of};

pub const DEFAULT_BRIEF_MINUTE: u32 = 5 * 60;
pub const LIST_LIMIT: usize = 10;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BriefFact {
    pub id: i64,
    pub kind: String,
    pub text: String,
    /// "me" for the user, else the name as written.
    pub owner: Option<String>,
    pub counterparty: Option<String>,
    pub due_at: Option<i64>,
    pub due_text: Option<String>,
    pub status: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BriefDay {
    pub summary_id: i64,
    pub text: String,
    pub highlights: Vec<Highlight>,
    pub stale: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Brief {
    /// Local date, YYYY-MM-DD.
    pub day: String,
    pub generated_at: i64,
    pub opener: String,
    pub overdue: Vec<BriefFact>,
    pub due_today: Vec<BriefFact>,
    pub commitments_by_me: Vec<BriefFact>,
    pub commitments_to_me: Vec<BriefFact>,
    pub waiting_on: Vec<BriefFact>,
    /// Loops later evidence says are finished, waiting for the user's confirmation.
    pub proposed_done: Vec<BriefFact>,
    pub yesterday: Option<BriefDay>,
}

fn zoned(ms: i64, tz: &TimeZone) -> Zoned {
    Timestamp::from_millisecond(ms)
        .unwrap_or(Timestamp::UNIX_EPOCH)
        .to_zoned(tz.clone())
}

pub fn day_key(now: i64, tz: &TimeZone) -> String {
    zoned(now, tz).strftime("%Y-%m-%d").to_string()
}

/// The day a brief should be made for now, if any: local time is at or after `brief_minute`
/// (minutes after midnight) and today has no brief yet.
pub fn due_day(conn: &Connection, now: i64, tz: &TimeZone, brief_minute: u32) -> Result<Option<String>, StoreError> {
    let z = zoned(now, tz);
    let minute = u32::try_from(i32::from(z.hour()) * 60 + i32::from(z.minute())).unwrap_or(0);
    if minute < brief_minute {
        return Ok(None);
    }
    let day = day_key(now, tz);
    Ok(summaries::get_brief(conn, &day)?.is_none().then_some(day))
}

fn to_brief(f: &FactRow, me: Option<i64>) -> BriefFact {
    let name = |id: Option<i64>, text: &Option<tsr_core::Sensitive<String>>| {
        if id.is_some() && id == me {
            Some("me".to_string())
        } else {
            text.as_ref().map(|t| t.expose().clone())
        }
    };
    BriefFact {
        id: f.id,
        kind: f.kind.clone(),
        text: f.text.expose().clone(),
        owner: name(f.owner_id, &f.owner_text),
        counterparty: name(f.counterparty_id, &f.counterparty_text),
        due_at: f.due_at,
        due_text: f.due_text.as_ref().map(|d| d.expose().clone()),
        status: f.status.clone(),
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// Build today's brief from the store (no model call).
pub fn assemble(conn: &Connection, now: i64, tz: &TimeZone) -> Result<Brief, StoreError> {
    let me = people::self_person(conn)?;
    let today = period_of(Level::Day, now, tz);
    let mut b = Brief {
        day: day_key(now, tz),
        generated_at: now,
        opener: String::new(),
        overdue: Vec::new(),
        due_today: Vec::new(),
        commitments_by_me: Vec::new(),
        commitments_to_me: Vec::new(),
        waiting_on: Vec::new(),
        proposed_done: Vec::new(),
        yesterday: None,
    };
    let mut all: Vec<FactRow> = facts::all(conn)?
        .into_iter()
        .filter(|f| matches!(f.kind.as_str(), "task" | "commitment" | "question"))
        .collect();
    all.sort_by_key(|f| (f.due_at.unwrap_or(i64::MAX), f.first_seen, f.id));
    for f in &all {
        let mine = f.owner_id.is_some() && f.owner_id == me;
        let to_me = f.counterparty_id.is_some() && f.counterparty_id == me;
        match f.status.as_str() {
            "proposed_done" => b.proposed_done.push(to_brief(f, me)),
            "open" => {
                if let Some(d) = f.due_at {
                    if d < today.start {
                        b.overdue.push(to_brief(f, me));
                    } else if d < today.end {
                        b.due_today.push(to_brief(f, me));
                    }
                }
                if f.kind == "commitment" && mine {
                    b.commitments_by_me.push(to_brief(f, me));
                } else if f.kind == "commitment" && to_me {
                    b.commitments_to_me.push(to_brief(f, me));
                } else if f.kind == "question" && mine {
                    b.waiting_on.push(to_brief(f, me));
                }
            }
            _ => {}
        }
    }
    for list in [
        &mut b.overdue,
        &mut b.due_today,
        &mut b.commitments_by_me,
        &mut b.commitments_to_me,
        &mut b.waiting_on,
        &mut b.proposed_done,
    ] {
        list.truncate(LIST_LIMIT);
    }
    let y = zoned(today.start, tz)
        .checked_sub(1.day())
        .map(|z| z.timestamp().as_millisecond());
    if let Ok(y) = y {
        let yp = period_of(Level::Day, y, tz);
        if let Some(s) = summaries::get(conn, "day", yp.start)? {
            b.yesterday = Some(BriefDay {
                summary_id: s.id,
                text: s.text.expose().clone(),
                highlights: s.highlights.expose().clone(),
                stale: s.stale,
            });
        }
    }
    let mut parts = Vec::new();
    if !b.overdue.is_empty() {
        parts.push(format!("{} overdue", plural(b.overdue.len(), "item", "items")));
    }
    if !b.due_today.is_empty() {
        parts.push(format!("{} due today", plural(b.due_today.len(), "item", "items")));
    }
    if !b.commitments_by_me.is_empty() {
        parts.push(format!(
            "{} you made",
            plural(b.commitments_by_me.len(), "open commitment", "open commitments")
        ));
    }
    if !b.waiting_on.is_empty() {
        parts.push(format!(
            "{} waiting on a reply",
            plural(b.waiting_on.len(), "question", "questions")
        ));
    }
    if !b.proposed_done.is_empty() {
        parts.push(format!(
            "{} that may be done",
            plural(b.proposed_done.len(), "loop", "loops")
        ));
    }
    b.opener = if parts.is_empty() {
        "Nothing due or waiting today.".into()
    } else {
        format!("Today: {}.", parts.join(", "))
    };
    Ok(b)
}

/// Assemble and store today's brief.
pub fn generate(conn: &Connection, now: i64, tz: &TimeZone) -> Result<Brief, StoreError> {
    let b = assemble(conn, now, tz)?;
    let json = serde_json::to_string(&b).map_err(|_| StoreError::Invalid("brief"))?;
    summaries::put_brief(conn, &b.day, &json, now)?;
    Ok(b)
}

/// The stored brief for `day`, if any.
pub fn load(conn: &Connection, day: &str) -> Result<Option<Brief>, StoreError> {
    Ok(summaries::get_brief(conn, day)?.and_then(|(j, _)| serde_json::from_str(j.expose()).ok()))
}
