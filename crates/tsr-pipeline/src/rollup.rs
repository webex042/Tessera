//! Hour, day, week and month rollups (plan §11.1, §16.3).
//!
//! - **Periods** are local-time boundaries (DST-aware) stored as UTC ms.
//! - **Planning** ([`due`]) is derived from the store each time, so it is durable by construction:
//!   an hour with activity is due 5 minutes after it closes; a day once it ends (or after 22:00
//!   when the user has been idle 10 minutes); a week or month once it ends. A parent waits until
//!   every child with activity has a fresh summary. Idle hours get no summary.
//! - **Inputs** are item summaries, titles and the facts first seen in the period (hour), or the
//!   child summaries (day, week, month). Content is escaped into a `<memory>` block.
//! - **Outputs** are validated by code: strict schema, highlight ids must be entry ids of this
//!   period (invalid ids stripped, empty highlights dropped), word limits per level, redaction,
//!   and no sentence that looks injected or tells the reader what to do.
//! - Without a model, or when the model fails twice, an **extractive** summary is written by code
//!   (model `extractive`) so every active period still has a rollup; it is regenerated with the
//!   model later only if it goes stale.
//! - Writing a summary marks the summaries above it stale; deleting an item marks every summary
//!   covering it stale (database trigger). Stale summaries are regenerated finest level first.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use jiff::tz::TimeZone;
use jiff::{Timestamp, ToSpan, Zoned};
use regex::Regex;
use rusqlite::Connection;
use serde::Deserialize;
use tsr_core::Sensitive;
use tsr_guard::redact::{Options, redact};
use tsr_infer::cancel::CancelReason;
use tsr_infer::cancel::CancelToken;
use tsr_infer::client::{ChatModel, ChatRequest, ClientError};
use tsr_infer::http::HttpError;
use tsr_infer::prompts::{ROLLUP_V1, escape_content, render};
use tsr_store::StoreError;
use tsr_store::summaries::{self, Highlight, NewSummary, SummaryRow};
use tsr_store::{facts, items, people};

use crate::guard::{looks_injected, obligation};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Level {
    Hour,
    Day,
    Week,
    Month,
}

impl Level {
    pub const ALL: [Level; 4] = [Level::Hour, Level::Day, Level::Week, Level::Month];

    pub fn as_str(self) -> &'static str {
        match self {
            Level::Hour => "hour",
            Level::Day => "day",
            Level::Week => "week",
            Level::Month => "month",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Level::ALL.into_iter().find(|l| l.as_str() == s)
    }

    /// Plan §11.1 word limits.
    pub fn word_limit(self) -> usize {
        match self {
            Level::Hour => 120,
            Level::Day => 200,
            Level::Week => 250,
            Level::Month => 300,
        }
    }

    pub fn child(self) -> Option<Level> {
        match self {
            Level::Hour => None,
            Level::Day => Some(Level::Hour),
            Level::Week => Some(Level::Day),
            Level::Month => Some(Level::Week),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Period {
    pub level: Level,
    pub start: i64,
    pub end: i64,
}

fn zoned(ms: i64, tz: &TimeZone) -> Zoned {
    Timestamp::from_millisecond(ms)
        .unwrap_or(Timestamp::UNIX_EPOCH)
        .to_zoned(tz.clone())
}

fn ms(z: &Zoned) -> i64 {
    z.timestamp().as_millisecond()
}

/// The `level` period containing `at_ms` in `tz`.
pub fn period_of(level: Level, at_ms: i64, tz: &TimeZone) -> Period {
    let z = zoned(at_ms, tz);
    let (start, end) = match level {
        // Truncate in absolute time: in a repeated DST hour, rebuilding "01:00" from fields would
        // pick the first 01:00 and miss items in the second one. Offsets are whole minutes.
        Level::Hour => {
            let into = i64::from(z.minute()) * 60_000 + i64::from(z.second()) * 1_000 + i64::from(z.millisecond());
            (at_ms - into, at_ms - into + 3_600_000)
        }
        Level::Day => {
            let s = z.start_of_day().unwrap_or_else(|_| z.clone());
            let e = s
                .checked_add(1.day())
                .and_then(|d| d.start_of_day())
                .unwrap_or_else(|_| s.clone());
            (ms(&s), ms(&e))
        }
        Level::Week => {
            let back = i64::from(z.weekday().to_monday_zero_offset());
            let s = z
                .checked_sub(back.days())
                .and_then(|d| d.start_of_day())
                .unwrap_or_else(|_| z.clone());
            let e = s
                .checked_add(7.days())
                .and_then(|d| d.start_of_day())
                .unwrap_or_else(|_| s.clone());
            (ms(&s), ms(&e))
        }
        Level::Month => {
            let s = z
                .first_of_month()
                .and_then(|d| d.start_of_day())
                .unwrap_or_else(|_| z.clone());
            let e = s
                .checked_add(1.month())
                .and_then(|d| d.start_of_day())
                .unwrap_or_else(|_| s.clone());
            (ms(&s), ms(&e))
        }
    };
    Period { level, start, end }
}

/// Human label for prompts and the UI, e.g. "Tuesday 27 October 2026, 14:00–15:00".
pub fn label(p: &Period, tz: &TimeZone) -> String {
    let s = zoned(p.start, tz);
    let e = zoned(p.end, tz);
    match p.level {
        Level::Hour => format!(
            "{}, {}–{}",
            s.strftime("%A %-d %B %Y"),
            s.strftime("%H:%M"),
            e.strftime("%H:%M")
        ),
        Level::Day => s.strftime("%A %-d %B %Y").to_string(),
        Level::Week => format!("week of Monday {}", s.strftime("%-d %B %Y")),
        Level::Month => s.strftime("%B %Y").to_string(),
    }
}

pub const HOUR_GRACE_MS: i64 = 5 * 60_000;
/// How far back planning looks for activity without stale summaries (a month plus a week).
pub const LOOKBACK_MS: i64 = 40 * 86_400_000;
pub const EVENING_HOUR: i8 = 22;
pub const DAY_IDLE_MS: u64 = 10 * 60_000;

fn fresh(conn: &Connection, p: &Period) -> Result<bool, StoreError> {
    Ok(summaries::get(conn, p.level.as_str(), p.start)?.is_some_and(|s| !s.stale))
}

/// Periods to (re)generate now, finest level first.
pub fn due(conn: &Connection, now: i64, tz: &TimeZone, idle_ms: u64) -> Result<Vec<Period>, StoreError> {
    let mut active: BTreeMap<Level, BTreeSet<Period>> = BTreeMap::new();
    for it in items::items_between(conn, now - LOOKBACK_MS, now)? {
        for l in Level::ALL {
            active.entry(l).or_default().insert(period_of(l, it.started_at, tz));
        }
    }
    // Stale summaries outside the lookback window (old deletions) are regenerated too.
    for s in summaries::stale(conn, 500)? {
        if let Some(l) = Level::parse(&s.level) {
            active.entry(l).or_default().insert(Period {
                level: l,
                start: s.period_start,
                end: s.period_end,
            });
        }
    }
    let now_z = zoned(now, tz);
    let mut out = Vec::new();
    let mut ready: BTreeSet<Period> = BTreeSet::new(); // fresh, or due in this plan (children first)
    for l in Level::ALL {
        for p in active.get(&l).into_iter().flatten() {
            let is_fresh = fresh(conn, p)?;
            let closed = match l {
                Level::Hour => p.end + HOUR_GRACE_MS <= now,
                Level::Day => {
                    p.end <= now
                        || (now >= p.start
                            && now_z.hour() >= EVENING_HOUR
                            && idle_ms >= DAY_IDLE_MS
                            && period_of(Level::Day, now, tz) == *p)
                }
                Level::Week | Level::Month => p.end <= now,
            };
            if !closed {
                continue;
            }
            let children_ready = match l.child() {
                None => true,
                Some(c) => active
                    .get(&c)
                    .into_iter()
                    .flatten()
                    .filter(|k| overlaps(k, p) && (k.end + if c == Level::Hour { HOUR_GRACE_MS } else { 0 }) <= now)
                    .all(|k| ready.contains(k)),
            };
            if is_fresh {
                ready.insert(*p);
            } else if children_ready {
                out.push(*p);
                ready.insert(*p);
            }
        }
    }
    Ok(out)
}

fn overlaps(a: &Period, b: &Period) -> bool {
    a.start < b.end && a.end > b.start
}

pub struct RollupCtx<'a> {
    pub tz: &'a TimeZone,
    pub redact: Options,
    pub model_id: &'a str,
    pub now_ms: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Generated {
    /// Written by the model (prompt `rollup/v1`).
    Model {
        summary_id: i64,
    },
    /// Written by code: no model, or the model's output failed validation twice.
    Extractive {
        summary_id: i64,
        model_failed: bool,
    },
    /// The period has no activity left (items deleted): its summary was removed.
    Removed,
    /// A child period has no fresh summary yet.
    NotReady,
    Cancelled,
    /// Stopped for a higher-priority job; nothing written.
    Preempted,
    /// The model process failed (not its output): nothing written, retry later.
    ModelUnavailable,
}

#[derive(Debug, thiserror::Error)]
pub enum RollupError {
    #[error("store: {0}")]
    Store(#[from] StoreError),
}

/// One `<memory>` entry and the ids a highlight may cite through it.
struct Entry {
    id: i64,
    line: String,
}

struct Input {
    entries: Vec<Entry>,
    item_ids: Vec<i64>,
    child_ids: Vec<i64>,
    /// child summary id → the items its highlights rest on (day and up).
    child_items: BTreeMap<i64, Vec<i64>>,
    children: Vec<SummaryRow>,
    items: Vec<items::ItemBrief>,
}

fn hhmm(ms: i64, tz: &TimeZone) -> String {
    zoned(ms, tz).strftime("%H:%M").to_string()
}

fn gather(conn: &Connection, p: &Period, tz: &TimeZone) -> Result<Input, StoreError> {
    let mut input = Input {
        entries: Vec::new(),
        item_ids: Vec::new(),
        child_ids: Vec::new(),
        child_items: BTreeMap::new(),
        children: Vec::new(),
        items: Vec::new(),
    };
    match p.level.child() {
        None => {
            let me = people::self_person(conn)?;
            for it in items::items_between(conn, p.start, p.end)? {
                let mut line = format!("[id {}] {} {}", it.id, hhmm(it.started_at, tz), escape_content(&it.app));
                if let Some(t) = &it.title {
                    line.push_str(&format!(" — \"{}\"", escape_content(t.expose())));
                }
                if let Some(s) = &it.summary {
                    line.push_str(&format!(" — summary: {}", escape_content(s.expose())));
                }
                for f in facts::for_item(conn, it.id)? {
                    if f.first_seen < p.start || f.first_seen >= p.end || f.status == "needs_review" {
                        continue;
                    }
                    let owner = if f.owner_id.is_some() && f.owner_id == me {
                        "me".to_string()
                    } else {
                        f.owner_text
                            .as_ref()
                            .map(|t| t.expose().clone())
                            .unwrap_or_else(|| "someone".into())
                    };
                    let to = f
                        .counterparty_text
                        .as_ref()
                        .map(|t| format!(" → {}", t.expose()))
                        .unwrap_or_default();
                    let due = f
                        .due_text
                        .as_ref()
                        .map(|d| format!(" (due {})", d.expose()))
                        .unwrap_or_default();
                    line.push_str(&format!(
                        "\n    fact: {}, {}{}: {}{}",
                        f.kind,
                        escape_content(&owner),
                        escape_content(&to),
                        escape_content(f.text.expose()),
                        escape_content(&due)
                    ));
                }
                input.item_ids.push(it.id);
                input.entries.push(Entry { id: it.id, line });
                input.items.push(it);
            }
        }
        Some(c) => {
            let children: Vec<SummaryRow> = summaries::in_range(conn, c.as_str(), p.start - 7 * 86_400_000, p.end)?
                .into_iter()
                .filter(|s| s.period_start < p.end && s.period_end > p.start)
                .collect();
            for ch in &children {
                let cp = Period {
                    level: c,
                    start: ch.period_start,
                    end: ch.period_end,
                };
                let mut line = format!(
                    "[id {}] {}: {}",
                    ch.id,
                    label(&cp, tz),
                    escape_content(ch.text.expose())
                );
                let mut its: BTreeSet<i64> = BTreeSet::new();
                for h in ch.highlights.expose() {
                    line.push_str(&format!("\n    - {}", escape_content(&h.text)));
                    its.extend(h.item_ids.iter().copied());
                }
                if its.is_empty() {
                    its.extend(summaries::source_items(conn, ch.id)?.into_iter().take(10));
                }
                input.child_items.insert(ch.id, its.into_iter().collect());
                input.child_ids.push(ch.id);
                input.entries.push(Entry { id: ch.id, line });
            }
            input.children = children;
        }
    }
    Ok(input)
}

/// Approximate input budget (plan: ≤ 3 000 tokens; ~4 characters per token).
pub const INPUT_BUDGET_CHARS: usize = 12_000;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawOut {
    summary: String,
    highlights: Vec<RawHighlight>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawHighlight {
    text: String,
    ids: Vec<i64>,
}

/// Addressed to the reader.
static TO_READER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\b(you|your)\b").expect("regex"));

/// An instruction opener ("Send the…", "Please…").
static IMPERATIVE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^\W*(please|remember|don't forget|do not forget|make sure|send|pay|transfer|wire|call|click|open|reply|forward|approve)\b")
        .expect("regex")
});

/// Rollups may describe obligations ("Ravi needs the sheet by Friday") but never instruct the
/// reader, and never carry text that looks like an injected instruction.
pub fn sentence_ok(s: &str) -> bool {
    !looks_injected(s) && !IMPERATIVE.is_match(s) && !(TO_READER.is_match(s) && obligation(s))
}

fn sentences(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let chars: Vec<char> = s.chars().collect();
    for (i, ch) in chars.iter().enumerate() {
        cur.push(*ch);
        let end = matches!(ch, '.' | '!' | '?') && chars.get(i + 1).is_none_or(|n| n.is_whitespace());
        if end || *ch == '\n' {
            let t = cur.trim().to_string();
            if !t.is_empty() {
                out.push(t);
            }
            cur.clear();
        }
    }
    let t = cur.trim().to_string();
    if !t.is_empty() {
        out.push(t);
    }
    out
}

/// Redact, drop unsafe sentences, and cut at a sentence boundary within `words` words.
fn clean_text(s: &str, words: usize, max_chars: usize, opts: Options) -> String {
    let r = redact(s, opts).text;
    let mut out = String::new();
    let mut n = 0;
    for sent in sentences(&r).into_iter().filter(|x| sentence_ok(x)) {
        let w = sent.split_whitespace().count();
        if n + w > words || out.chars().count() + sent.chars().count() + 1 > max_chars {
            if n == 0 {
                // A single over-long sentence: keep its first words.
                let cut: Vec<&str> = sent.split_whitespace().take(words).collect();
                out = cut.join(" ");
                while out.chars().count() > max_chars {
                    out.pop();
                }
            }
            break;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&sent);
        n += w;
    }
    out
}

/// Validate model output for a period. `Err` = schema or content invalid (retry / fall back).
fn validate(raw: &str, level: Level, input: &Input, opts: Options) -> Result<(String, Vec<Highlight>), &'static str> {
    let out: RawOut = serde_json::from_str(raw.trim()).map_err(|_| "schema")?;
    if out.summary.chars().count() > 900 || out.highlights.len() > 6 {
        return Err("schema");
    }
    let summary = clean_text(&out.summary, level.word_limit(), 900, opts);
    if summary.is_empty() {
        return Err("empty");
    }
    let valid: BTreeSet<i64> = input.entries.iter().map(|e| e.id).collect();
    let mut hs = Vec::new();
    for h in out.highlights {
        if h.text.chars().count() > 160 {
            return Err("schema");
        }
        let ids: Vec<i64> = h
            .ids
            .into_iter()
            .filter(|i| valid.contains(i))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let text = clean_text(&h.text, 40, 160, opts);
        if ids.is_empty() || text.is_empty() {
            continue; // invalid ids stripped (plan §11.1); nothing left to cite
        }
        hs.push(to_highlight(text, ids, level, input));
    }
    Ok((summary, hs))
}

fn to_highlight(text: String, ids: Vec<i64>, level: Level, input: &Input) -> Highlight {
    if level == Level::Hour {
        Highlight {
            text,
            item_ids: ids,
            child_ids: Vec::new(),
        }
    } else {
        let mut its = BTreeSet::new();
        for c in &ids {
            its.extend(input.child_items.get(c).into_iter().flatten().copied());
        }
        Highlight {
            text,
            item_ids: its.into_iter().collect(),
            child_ids: ids,
        }
    }
}

/// Code-only summary: item summaries/titles (hour) or child summaries' first sentences.
fn extractive(level: Level, input: &Input, opts: Options) -> (String, Vec<Highlight>) {
    let mut parts: Vec<String> = Vec::new();
    let mut hs = Vec::new();
    if level == Level::Hour {
        for it in &input.items {
            let s = it
                .summary
                .as_ref()
                .map(|s| s.expose().clone())
                .or_else(|| it.title.as_ref().map(|t| format!("{}: {}.", it.app, t.expose())))
                .unwrap_or_else(|| format!("Used {}.", it.app));
            parts.push(s.clone());
            if hs.len() < 6 {
                let text = clean_text(&s, 30, 160, opts);
                if !text.is_empty() {
                    hs.push(to_highlight(text, vec![it.id], level, input));
                }
            }
        }
    } else {
        for ch in &input.children {
            if let Some(first) = sentences(ch.text.expose()).into_iter().next() {
                parts.push(first);
            }
            if hs.len() < 6
                && let Some(h) = ch.highlights.expose().first()
            {
                let text = clean_text(&h.text, 30, 160, opts);
                if !text.is_empty() {
                    hs.push(to_highlight(text, vec![ch.id], level, input));
                }
            }
        }
    }
    (clean_text(&parts.join(" "), level.word_limit(), 900, opts), hs)
}

fn request(p: &Period, entries: &[&str], tz: &TimeZone, temperature: f32) -> ChatRequest {
    let pr = &*ROLLUP_V1;
    let system = render(pr.system, &[("level", p.level.as_str())]).expect("template");
    let user = render(
        pr.user,
        &[
            ("period_label", &label(p, tz)),
            (
                "entries",
                &entries.join(
                    "
",
                ),
            ),
        ],
    )
    .expect("template");
    ChatRequest {
        system: Sensitive::new(system),
        user: Sensitive::new(user),
        image_png: None,
        temperature,
        max_tokens: 900,
        json_schema: pr.schema.clone(),
        stream: true,
        seed: Some(7),
    }
}

/// Split entries into chunks that fit the input budget (an entry never splits).
fn chunks(entries: &[Entry]) -> Vec<Vec<&Entry>> {
    let mut out: Vec<Vec<&Entry>> = vec![Vec::new()];
    let mut size = 0;
    for e in entries {
        if size + e.line.len() + 1 > INPUT_BUDGET_CHARS && !out.last().is_some_and(Vec::is_empty) {
            out.push(Vec::new());
            size = 0;
        }
        size += e.line.len() + 1;
        out.last_mut().expect("chunk").push(e);
    }
    out
}

enum Call {
    Ok(String, Vec<Highlight>),
    /// Output invalid twice: fall back to an extractive summary.
    Failed,
    Cancelled,
    Preempted,
    /// Transport or process failure: retry later, write nothing.
    Unavailable,
}

/// One validated model call over `entries` (two attempts, as in extraction).
fn call(
    m: &dyn ChatModel,
    p: &Period,
    entries: &[&Entry],
    input: &Input,
    ctx: &RollupCtx<'_>,
    cancel: &CancelToken,
) -> Call {
    let lines: Vec<&str> = entries.iter().map(|e| e.line.as_str()).collect();
    // Only the ids shown in this call may be cited.
    let shown = Input {
        entries: entries
            .iter()
            .map(|e| Entry {
                id: e.id,
                line: String::new(),
            })
            .collect(),
        item_ids: Vec::new(),
        child_ids: Vec::new(),
        child_items: input.child_items.clone(),
        children: Vec::new(),
        items: Vec::new(),
    };
    for t in [0.1f32, 0.0] {
        if cancel.is_cancelled() {
            return Call::Cancelled;
        }
        match m.chat(&request(p, &lines, ctx.tz, t), cancel) {
            Ok(r) => {
                if let Ok((s, h)) = validate(r.content.expose(), p.level, &shown, ctx.redact) {
                    return Call::Ok(s, h);
                }
            }
            Err(ClientError::Http(HttpError::Cancelled(CancelReason::Preempted))) => return Call::Preempted,
            Err(ClientError::Http(HttpError::Cancelled(_))) => return Call::Cancelled,
            Err(ClientError::BadResponse(_)) => {} // counts as an invalid output
            Err(_) => return Call::Unavailable,
        }
    }
    Call::Failed
}

/// Model summary of the whole period: one call if the input fits, else map-reduce — each chunk
/// summarised on its own, then one call over the partial summaries writes the final summary,
/// and the highlights are taken (by code) from the validated partial highlights.
fn model_summary(m: &dyn ChatModel, p: &Period, input: &Input, ctx: &RollupCtx<'_>, cancel: &CancelToken) -> Call {
    let parts = chunks(&input.entries);
    if parts.len() == 1 {
        return call(m, p, &parts[0], input, ctx, cancel);
    }
    let mut partials: Vec<(String, Vec<Highlight>)> = Vec::new();
    for c in &parts {
        match call(m, p, c, input, ctx, cancel) {
            Call::Ok(s, h) => partials.push((s, h)),
            other => return other,
        }
    }
    let reduce: Vec<Entry> = partials
        .iter()
        .enumerate()
        .map(|(i, (s, _))| Entry {
            id: -(i as i64) - 1, // not citable: highlights come from the partials
            line: format!("[part {}] {}", i + 1, escape_content(s)),
        })
        .collect();
    let refs: Vec<&Entry> = reduce.iter().collect();
    let summary = match call(m, p, &refs, input, ctx, cancel) {
        Call::Ok(s, _) => s,
        other => return other,
    };
    // Round-robin over the parts so the highlights cover the whole period.
    let mut hs = Vec::new();
    let mut idx = 0;
    while hs.len() < 6 && partials.iter().any(|(_, h)| idx < h.len()) {
        for (_, h) in &partials {
            if let Some(x) = h.get(idx)
                && hs.len() < 6
            {
                hs.push(x.clone());
            }
        }
        idx += 1;
    }
    Call::Ok(summary, hs)
}

/// Generate (or regenerate) the summary of one period.
pub fn run_period(
    conn: &mut Connection,
    p: &Period,
    model: Option<&dyn ChatModel>,
    ctx: &RollupCtx<'_>,
    cancel: &CancelToken,
) -> Result<Generated, RollupError> {
    let input = gather(conn, p, ctx.tz)?;
    if input.entries.is_empty() {
        // Nothing left in this period (items deleted): no summary, and its parents go stale.
        let had = summaries::get(conn, p.level.as_str(), p.start)?.is_some();
        conn.execute(
            "DELETE FROM summaries WHERE level = ?1 AND period_start = ?2",
            rusqlite::params![p.level.as_str(), p.start],
        )
        .map_err(StoreError::from)?;
        if had {
            mark_parents_stale(conn, p)?;
        }
        return Ok(Generated::Removed);
    }
    let mut written: Option<(String, Vec<Highlight>, &str, &str)> = None;
    let mut model_failed = false;
    if let Some(m) = model {
        match model_summary(m, p, &input, ctx, cancel) {
            Call::Ok(s, h) => written = Some((s, h, ROLLUP_V1.ver, ctx.model_id)),
            Call::Failed => model_failed = true,
            Call::Cancelled => return Ok(Generated::Cancelled),
            Call::Preempted => return Ok(Generated::Preempted),
            Call::Unavailable => return Ok(Generated::ModelUnavailable),
        }
    }
    let by_model = written.is_some();
    let (text, hs, ver, model_name) = written.unwrap_or_else(|| {
        let (s, h) = extractive(p.level, &input, ctx.redact);
        (s, h, "extractive/v1", "extractive")
    });
    let id = summaries::upsert(
        conn,
        &NewSummary {
            level: p.level.as_str(),
            period_start: p.start,
            period_end: p.end,
            text: &text,
            highlights: &hs,
            item_ids: &input.item_ids,
            child_ids: &input.child_ids,
            prompt_ver: ver,
            model: model_name,
            now: ctx.now_ms,
        },
    )?;
    mark_parents_stale(conn, p)?;
    Ok(if by_model {
        Generated::Model { summary_id: id }
    } else {
        Generated::Extractive {
            summary_id: id,
            model_failed,
        }
    })
}

/// A new or removed summary makes the coarser summaries over it out of date.
fn mark_parents_stale(conn: &Connection, p: &Period) -> Result<(), StoreError> {
    conn.execute(
        "UPDATE summaries SET stale = 1
          WHERE period_start < ?2 AND period_end > ?1
            AND CASE level WHEN 'hour' THEN 0 WHEN 'day' THEN 1 WHEN 'week' THEN 2 ELSE 3 END > ?3",
        rusqlite::params![p.start, p.end, p.level as i64],
    )?;
    Ok(())
}

/// Run every due period now (tests, replays, and catching up after a long gap).
pub fn run_all_due(
    conn: &mut Connection,
    model: Option<&dyn ChatModel>,
    ctx: &RollupCtx<'_>,
    idle_ms: u64,
    cancel: &CancelToken,
) -> Result<Vec<(Period, Generated)>, RollupError> {
    let mut done = Vec::new();
    // Each pass handles one level's worth of newly ready periods; parents become due next pass.
    for _ in 0..8 {
        let due = due(conn, ctx.now_ms, ctx.tz, idle_ms)?;
        if due.is_empty() {
            break;
        }
        for p in due {
            let g = run_period(conn, &p, model, ctx, cancel)?;
            if matches!(
                g,
                Generated::Cancelled | Generated::Preempted | Generated::ModelUnavailable
            ) {
                done.push((p, g));
                return Ok(done);
            }
            done.push((p, g));
        }
    }
    Ok(done)
}
