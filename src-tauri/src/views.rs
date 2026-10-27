//! App core: the data behind the notch views, as plain Rust (plan §14). Tauri commands in
//! `lib.rs` are thin wrappers, so everything here is tested without a window.
//!
//! - Every input is validated here (lengths, enums, ids); errors carry a stable code and a
//!   message key, never content.
//! - Changing a fact's status needs a single-use approval token issued by Rust for exactly that
//!   (fact, status) pair, valid for two minutes (plan §6.5). The UI cannot make one up.
//! - Day boundaries use the user's time zone, passed in (tests use fixed zones).

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

pub const TOKEN_TTL: Duration = Duration::from_secs(120);
pub const MAX_QUERY_CHARS: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, thiserror::Error)]
#[error("{code}")]
pub struct ApiError {
    pub code: &'static str,
    /// Localisable message key for the UI.
    pub message: &'static str,
}

impl ApiError {
    pub const fn new(code: &'static str, message: &'static str) -> Self {
        ApiError { code, message }
    }
}

pub const E_INVALID: ApiError = ApiError::new("invalid_input", "error.invalid_input");
pub const E_NOT_FOUND: ApiError = ApiError::new("not_found", "error.not_found");
pub const E_STORE: ApiError = ApiError::new("store_error", "error.store");
pub const E_TOKEN: ApiError = ApiError::new("approval_invalid", "error.approval_invalid");

impl From<tsr_store::StoreError> for ApiError {
    fn from(_: tsr_store::StoreError) -> Self {
        E_STORE
    }
}

impl From<rusqlite::Error> for ApiError {
    fn from(_: rusqlite::Error) -> Self {
        E_STORE
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FactDto {
    pub id: i64,
    pub kind: String,
    pub text: String,
    pub status: String,
    pub owner: Option<String>,
    pub counterparty: Option<String>,
    pub due_text: Option<String>,
    pub due_at: Option<i64>,
    pub due_precision: Option<String>,
    pub due_ambiguous: bool,
    pub confidence: String,
    pub first_seen: i64,
    pub last_seen: i64,
    /// The item the first evidence quote comes from (drill-down / OpenSource).
    pub item_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ItemCard {
    pub item_id: i64,
    pub app: String,
    pub title: Option<String>,
    pub started_at: i64,
    pub ended_at: i64,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct HourBucket {
    pub hour: u8,
    pub start: i64,
    pub items: Vec<ItemCard>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Today {
    pub day_start: i64,
    pub overdue: Vec<FactDto>,
    pub due_today: Vec<FactDto>,
    pub open_commitments: Vec<FactDto>,
    pub needs_review: Vec<FactDto>,
    pub timeline: Vec<HourBucket>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SearchHit {
    pub item: ItemCard,
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ItemDetail {
    pub card: ItemCard,
    pub text: String,
    pub url: Option<String>,
    pub facts: Vec<FactDto>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Counts {
    pub open: u32,
    pub due_today: u32,
    pub overdue: u32,
    pub needs_review: u32,
    pub items_today: u32,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct DayRange {
    pub start: i64,
    pub end: i64,
}

/// Local day containing `now_ms` in `tz`.
pub fn day_range(now_ms: i64, tz: &jiff::tz::TimeZone) -> Result<DayRange, ApiError> {
    let z = jiff::Timestamp::from_millisecond(now_ms)
        .map_err(|_| E_INVALID)?
        .to_zoned(tz.clone());
    let start = z.start_of_day().map_err(|_| E_INVALID)?;
    let end = start
        .tomorrow()
        .map_err(|_| E_INVALID)?
        .start_of_day()
        .map_err(|_| E_INVALID)?;
    Ok(DayRange {
        start: start.timestamp().as_millisecond(),
        end: end.timestamp().as_millisecond(),
    })
}

fn person_name(c: &Connection, id: Option<i64>, text: Option<String>) -> Result<Option<String>, ApiError> {
    match id {
        Some(id) => Ok(c
            .query_row(
                "SELECT CASE WHEN is_self = 1 THEN 'me' ELSE display_name END FROM people WHERE id = ?1",
                [id],
                |r| r.get(0),
            )
            .optional()?),
        None => Ok(text),
    }
}

fn fact_dto(c: &Connection, f: tsr_store::facts::FactRow) -> Result<FactDto, ApiError> {
    let item_id: Option<i64> = c
        .query_row(
            "SELECT item_id FROM fact_evidence WHERE fact_id = ?1 ORDER BY added_at, item_id LIMIT 1",
            [f.id],
            |r| r.get(0),
        )
        .optional()?;
    Ok(FactDto {
        id: f.id,
        owner: person_name(c, f.owner_id, f.owner_text.map(|t| t.expose().clone()))?,
        counterparty: person_name(c, f.counterparty_id, f.counterparty_text.map(|t| t.expose().clone()))?,
        kind: f.kind,
        text: f.text.expose().clone(),
        status: f.status,
        due_text: f.due_text.map(|t| t.expose().clone()),
        due_at: f.due_at,
        due_precision: f.due_precision,
        due_ambiguous: f.due_ambiguous,
        confidence: f.confidence,
        first_seen: f.first_seen,
        last_seen: f.last_seen,
        item_id,
    })
}

fn card(r: &rusqlite::Row<'_>) -> rusqlite::Result<ItemCard> {
    Ok(ItemCard {
        item_id: r.get(0)?,
        app: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
        title: r.get(2)?,
        started_at: r.get(3)?,
        ended_at: r.get(4)?,
        summary: r.get(5)?,
    })
}

const CARD_SQL: &str = "SELECT items.id, COALESCE(sources.app_name, sources.app_exe), sources.window_title,
            items.started_at, items.ended_at, items.summary
     FROM items JOIN sources ON sources.id = items.source_id";

#[derive(Debug, Clone, PartialEq, Eq)]
struct Pending {
    /// Sorted. One id for a single change; several for "dismiss all" on stale loops.
    fact_ids: Vec<i64>,
    status: String,
    issued: Instant,
}

/// Read side of the app: one read connection (WAL keeps it unblocked by the writer).
pub struct Core {
    conn: Mutex<Connection>,
    tokens: Mutex<HashMap<String, Pending>>,
}

/// Statuses the user may set (plan §6.5 `SetFactStatus`). `proposed_done` is set by code only.
pub const USER_STATUSES: [&str; 3] = ["open", "done", "dropped"];

impl Core {
    pub fn new(conn: Connection) -> Self {
        Core {
            conn: Mutex::new(conn),
            tokens: Mutex::new(HashMap::new()),
        }
    }

    fn c(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn facts_where(&self, sql_where: &str, p: &[&dyn rusqlite::ToSql]) -> Result<Vec<FactDto>, ApiError> {
        let c = self.c();
        let ids: Vec<i64> = c
            .prepare(&format!("SELECT id FROM facts WHERE {sql_where}"))?
            .query_map(p, |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        let mut out = Vec::new();
        for id in ids {
            if let Some(f) = tsr_store::facts::get(&c, id)? {
                out.push(fact_dto(&c, f)?);
            }
        }
        Ok(out)
    }

    pub fn counts(&self, now_ms: i64, tz: &jiff::tz::TimeZone) -> Result<Counts, ApiError> {
        let d = day_range(now_ms, tz)?;
        let c = self.c();
        let q = |sql: &str, p: &[&dyn rusqlite::ToSql]| -> Result<u32, ApiError> {
            Ok(c.query_row(sql, p, |r| r.get::<_, i64>(0))? as u32)
        };
        Ok(Counts {
            open: q(
                "SELECT count(*) FROM facts WHERE status = 'open' AND kind IN ('task','commitment')",
                &[],
            )?,
            due_today: q(
                "SELECT count(*) FROM facts WHERE status = 'open' AND due_at >= ?1 AND due_at < ?2",
                &[&d.start, &d.end],
            )?,
            overdue: q(
                "SELECT count(*) FROM facts WHERE status = 'open' AND due_at < ?1 AND due_at IS NOT NULL",
                &[&now_ms],
            )?,
            needs_review: q("SELECT count(*) FROM facts WHERE status = 'needs_review'", &[])?,
            items_today: q(
                "SELECT count(*) FROM items WHERE started_at >= ?1 AND started_at < ?2",
                &[&d.start, &d.end],
            )?,
        })
    }

    pub fn today(&self, now_ms: i64, tz: &jiff::tz::TimeZone) -> Result<Today, ApiError> {
        let d = day_range(now_ms, tz)?;
        Ok(Today {
            day_start: d.start,
            overdue: self.facts_where(
                "status = 'open' AND due_at IS NOT NULL AND due_at < ?1 ORDER BY due_at",
                &[&now_ms],
            )?,
            due_today: self.facts_where(
                "status = 'open' AND due_at >= ?1 AND due_at < ?2 ORDER BY due_at",
                &[&now_ms.max(d.start), &d.end],
            )?,
            open_commitments: self.facts_where(
                "status = 'open' AND kind = 'commitment' AND due_at IS NULL ORDER BY last_seen DESC LIMIT 20",
                &[],
            )?,
            needs_review: self.facts_where("status = 'needs_review' ORDER BY last_seen DESC LIMIT 20", &[])?,
            timeline: self.timeline_day(now_ms, tz)?,
        })
    }

    /// Hour buckets of the local day containing `at_ms`; only hours with items are returned.
    pub fn timeline_day(&self, at_ms: i64, tz: &jiff::tz::TimeZone) -> Result<Vec<HourBucket>, ApiError> {
        let d = day_range(at_ms, tz)?;
        let c = self.c();
        let cards: Vec<ItemCard> = c
            .prepare(&format!(
                "{CARD_SQL} WHERE items.started_at >= ?1 AND items.started_at < ?2 ORDER BY items.started_at, items.id"
            ))?
            .query_map(params![d.start, d.end], card)?
            .collect::<Result<_, _>>()?;
        let mut buckets: Vec<HourBucket> = Vec::new();
        for it in cards {
            let z = jiff::Timestamp::from_millisecond(it.started_at)
                .map_err(|_| E_INVALID)?
                .to_zoned(tz.clone());
            let hour = z.hour() as u8;
            let start = z
                .with()
                .minute(0)
                .second(0)
                .subsec_nanosecond(0)
                .build()
                .map_err(|_| E_INVALID)?
                .timestamp()
                .as_millisecond();
            match buckets.last_mut() {
                // Keyed by the instant, so the repeated hour of a DST fold stays two buckets.
                Some(b) if b.start == start => b.items.push(it),
                _ => buckets.push(HourBucket {
                    hour,
                    start,
                    items: vec![it],
                }),
            }
        }
        Ok(buckets)
    }

    pub fn open_loops(&self, kind: Option<&str>) -> Result<Vec<FactDto>, ApiError> {
        match kind {
            None => self.facts_where(
                "status IN ('open','needs_review','proposed_done') AND kind IN ('task','commitment','question')
                 ORDER BY due_at IS NULL, due_at, last_seen DESC LIMIT 200",
                &[],
            ),
            Some(k) if tsr_store::facts::KINDS.contains(&k) => self.facts_where(
                "status IN ('open','needs_review','proposed_done') AND kind = ?1 ORDER BY due_at IS NULL, due_at, last_seen DESC LIMIT 200",
                &[&k],
            ),
            Some(_) => Err(E_INVALID),
        }
    }

    pub fn search(&self, query: &str, app: Option<&str>, limit: u32) -> Result<Vec<SearchHit>, ApiError> {
        if query.trim().is_empty() || query.chars().count() > MAX_QUERY_CHARS || limit == 0 || limit > 100 {
            return Err(E_INVALID);
        }
        let c = self.c();
        let hits = tsr_store::items::search_any(&c, query, limit * 3)?;
        let mut out = Vec::new();
        for h in hits {
            let it = c
                .prepare(&format!("{CARD_SQL} WHERE items.id = ?1"))?
                .query_row([h.item_id], card)?;
            if app.is_some_and(|a| !it.app.eq_ignore_ascii_case(a)) {
                continue;
            }
            out.push(SearchHit {
                item: it,
                snippet: h.snippet.expose().clone(),
            });
            if out.len() as u32 == limit {
                break;
            }
        }
        Ok(out)
    }

    pub fn item(&self, id: i64) -> Result<ItemDetail, ApiError> {
        let c = self.c();
        let card = c
            .prepare(&format!("{CARD_SQL} WHERE items.id = ?1"))?
            .query_row([id], card)
            .optional()?
            .ok_or(E_NOT_FOUND)?;
        let (text, url): (String, Option<String>) = c.query_row(
            "SELECT items.text, sources.url FROM items JOIN sources ON sources.id = items.source_id WHERE items.id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let facts = tsr_store::facts::for_item(&c, id)?
            .into_iter()
            .map(|f| fact_dto(&c, f))
            .collect::<Result<_, _>>()?;
        Ok(ItemDetail { card, text, url, facts })
    }

    /// Issue a single-use token for setting `fact_id` to `status`.
    pub fn propose_fact_status(&self, fact_id: i64, status: &str, now: Instant) -> Result<String, ApiError> {
        if !USER_STATUSES.contains(&status) {
            return Err(E_INVALID);
        }
        if tsr_store::facts::get(&self.c(), fact_id)?.is_none() {
            return Err(E_NOT_FOUND);
        }
        let token = new_token();
        let mut t = self.tokens.lock().unwrap_or_else(|p| p.into_inner());
        t.retain(|_, p| now.duration_since(p.issued) < TOKEN_TTL);
        t.insert(
            token.clone(),
            Pending {
                fact_ids: vec![fact_id],
                status: status.to_owned(),
                issued: now,
            },
        );
        Ok(token)
    }

    /// Apply a proposal. The token must exist, match exactly, be unexpired; it is consumed.
    pub fn set_fact_status(&self, fact_id: i64, status: &str, token: &str, now: Instant) -> Result<FactDto, ApiError> {
        let p = self
            .tokens
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(token)
            .ok_or(E_TOKEN)?;
        if p.fact_ids != [fact_id] || p.status != status || now.duration_since(p.issued) >= TOKEN_TTL {
            return Err(E_TOKEN);
        }
        let c = self.c();
        if !tsr_store::facts::set_status(&c, fact_id, status)? {
            return Err(E_NOT_FOUND);
        }
        fact_dto(&c, tsr_store::facts::get(&c, fact_id)?.ok_or(E_NOT_FOUND)?)
    }

    /// Issue a single-use token for setting *exactly* these facts to `status` ("dismiss all").
    pub fn propose_bulk_status(&self, fact_ids: &[i64], status: &str, now: Instant) -> Result<String, ApiError> {
        if !USER_STATUSES.contains(&status) || fact_ids.is_empty() || fact_ids.len() > MAX_BULK {
            return Err(E_INVALID);
        }
        let mut ids = fact_ids.to_vec();
        ids.sort_unstable();
        ids.dedup();
        {
            let c = self.c();
            for id in &ids {
                if tsr_store::facts::get(&c, *id)?.is_none() {
                    return Err(E_NOT_FOUND);
                }
            }
        }
        let token = new_token();
        let mut t = self.tokens.lock().unwrap_or_else(|p| p.into_inner());
        t.retain(|_, p| now.duration_since(p.issued) < TOKEN_TTL);
        t.insert(
            token.clone(),
            Pending {
                fact_ids: ids,
                status: status.to_owned(),
                issued: now,
            },
        );
        Ok(token)
    }

    /// Apply a bulk proposal: same rules as [`Core::set_fact_status`], all or nothing.
    pub fn set_bulk_status(&self, fact_ids: &[i64], status: &str, token: &str, now: Instant) -> Result<u32, ApiError> {
        let p = self
            .tokens
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(token)
            .ok_or(E_TOKEN)?;
        let mut ids = fact_ids.to_vec();
        ids.sort_unstable();
        ids.dedup();
        if p.fact_ids != ids || p.status != status || now.duration_since(p.issued) >= TOKEN_TTL {
            return Err(E_TOKEN);
        }
        let c = self.c();
        let tx = c.unchecked_transaction()?;
        for id in &ids {
            if !tsr_store::facts::set_status(&tx, *id, status)? {
                return Err(E_NOT_FOUND); // dropping `tx` rolls back
            }
        }
        tx.commit()?;
        Ok(ids.len() as u32)
    }

    /// Open loops in groups: proposed done (confirm), open, and "Still open?" (stale).
    pub fn loop_groups(&self, now_ms: i64) -> Result<LoopGroups, ApiError> {
        let c = self.c();
        let g = tsr_pipeline::loops::groups(&c, now_ms)?;
        let dto =
            |v: Vec<tsr_store::facts::FactRow>| v.into_iter().map(|f| fact_dto(&c, f)).collect::<Result<Vec<_>, _>>();
        Ok(LoopGroups {
            proposed_done: dto(g.proposed_done)?,
            open: dto(g.open)?,
            still_open: dto(g.still_open)?,
        })
    }

    /// Today's stored brief, or one assembled now (not stored) if the trigger has not run yet.
    pub fn brief(&self, now_ms: i64, tz: &jiff::tz::TimeZone) -> Result<tsr_pipeline::brief::Brief, ApiError> {
        let c = self.c();
        let day = tsr_pipeline::brief::day_key(now_ms, tz);
        match tsr_pipeline::brief::load(&c, &day)? {
            Some(b) => Ok(b),
            None => Ok(tsr_pipeline::brief::assemble(&c, now_ms, tz)?),
        }
    }

    /// A day (its hours), week (its days) or month (its weeks), with summaries where they exist.
    pub fn rollups(&self, level: &str, at_ms: i64, tz: &jiff::tz::TimeZone) -> Result<RollupView, ApiError> {
        use tsr_pipeline::rollup::{Level, label, period_of};
        let lvl = match Level::parse(level) {
            Some(l @ (Level::Day | Level::Week | Level::Month)) => l,
            _ => return Err(E_INVALID),
        };
        let p = period_of(lvl, at_ms, tz);
        let c = self.c();
        let child = lvl.child().ok_or(E_INVALID)?;
        let mut children = Vec::new();
        let mut t = period_of(child, p.start, tz);
        while t.start < p.end && children.len() < 40 {
            children.push(PeriodDto {
                level: child.as_str(),
                start: t.start,
                end: t.end,
                label: label(&t, tz),
                summary: summary_dto(&c, child.as_str(), t.start)?,
            });
            t = period_of(child, t.end, tz);
        }
        Ok(RollupView {
            period: PeriodDto {
                level: lvl.as_str(),
                start: p.start,
                end: p.end,
                label: label(&p, tz),
                summary: summary_dto(&c, lvl.as_str(), p.start)?,
            },
            children,
        })
    }

    /// Item cards for a highlight's ids (drill-down). Ids that no longer exist are skipped.
    pub fn items_by_ids(&self, ids: &[i64]) -> Result<Vec<ItemCard>, ApiError> {
        if ids.is_empty() || ids.len() > MAX_BULK {
            return Err(E_INVALID);
        }
        let c = self.c();
        let mut st = c.prepare(&format!("{CARD_SQL} WHERE items.id = ?1"))?;
        let mut out = Vec::new();
        for id in ids {
            if let Some(it) = st.query_row([id], card).optional()? {
                out.push(it);
            }
        }
        out.sort_by_key(|c| (c.started_at, c.item_id));
        Ok(out)
    }
}

pub const MAX_BULK: usize = 200;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LoopGroups {
    pub proposed_done: Vec<FactDto>,
    pub open: Vec<FactDto>,
    pub still_open: Vec<FactDto>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct HighlightDto {
    pub text: String,
    /// Only items that still exist.
    pub item_ids: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SummaryDto {
    pub id: i64,
    pub text: String,
    pub highlights: Vec<HighlightDto>,
    pub stale: bool,
    /// Model id, or "extractive" (written by code).
    pub model: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PeriodDto {
    pub level: &'static str,
    pub start: i64,
    pub end: i64,
    pub label: String,
    pub summary: Option<SummaryDto>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RollupView {
    pub period: PeriodDto,
    pub children: Vec<PeriodDto>,
}

fn summary_dto(c: &Connection, level: &str, start: i64) -> Result<Option<SummaryDto>, ApiError> {
    let Some(s) = tsr_store::summaries::get(c, level, start)? else {
        return Ok(None);
    };
    let mut highlights = Vec::new();
    for h in s.highlights.expose() {
        let item_ids = tsr_store::items::existing(c, &h.item_ids)?;
        if !item_ids.is_empty() {
            highlights.push(HighlightDto {
                text: h.text.clone(),
                item_ids,
            });
        }
    }
    Ok(Some(SummaryDto {
        id: s.id,
        text: s.text.expose().clone(),
        highlights,
        stale: s.stale,
        model: s.model,
    }))
}

fn new_token() -> String {
    #[cfg(windows)]
    let b = tsr_win::dpapi::random_bytes::<16>().expect("BCryptGenRandom");
    #[cfg(not(windows))]
    let b = {
        use std::io::Read;
        let mut b = [0u8; 16];
        std::fs::File::open("/dev/urandom")
            .and_then(|mut f| f.read_exact(&mut b))
            .expect("urandom");
        b
    };
    b.iter().map(|x| format!("{x:02x}")).collect()
}
