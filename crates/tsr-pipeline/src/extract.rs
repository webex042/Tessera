//! The extraction job (plan §8.2, §8.3): one item in, validated facts out.
//!
//! Stages (recorded in `jobs.stage`, content-free):
//!
//! 1. `authorize` — the item exists, is still pending, and its source is not blocked by the
//!    *current* blocklist (a rule added after capture still applies).
//! 2. `classify` — too-short items are skipped.
//! 3. `model` — prompt per ≤ 7 200-character slice (paragraph boundaries), JSON-schema output,
//!    streamed so a P0 job can preempt it. Preemption or cancellation stores nothing.
//! 4. `validate` — strict schema ([`crate::schema`]); on failure one retry at temperature 0,
//!    then the item is marked `failed` with nothing stored.
//! 5. `verify` — per fact: evidence in the item ([`crate::evidence`]), instruction class
//!    ([`crate::guard`]), owner named in the source, numbers present in the source, due text
//!    copied from the source and resolved ([`crate::dates`]).
//! 6. `redact` — second redaction pass over every generated field (tsr-guard), then suppression.
//! 7. `persist` — one transaction: people, de-duplication against open facts of the last 14 days
//!    (same kind and owner, compatible due date, word overlap ≥ 0.85), facts,
//!    evidence, the item's summary and state, the job's state. Any error rolls all of it back.
//!
//! Every rejected fact is counted by reason in the [`Report`]; nothing is dropped silently.

use std::collections::BTreeMap;

use rusqlite::Connection;
use tsr_core::Sensitive;
use tsr_guard::blocklist::{Blocklist, PrivateState, WindowFacts};
use tsr_guard::redact::{Options as RedactOptions, REDACT_VER, redact};
use tsr_infer::cancel::{CancelReason, CancelToken};
use tsr_infer::client::{ChatModel, ChatRequest, ClientError};
use tsr_infer::http::HttpError;
use tsr_infer::prompts::{EXTRACTION_V1, escape_content, render};
use tsr_store::StoreError;
use tsr_store::facts::{self, NewEvidence, NewFact};
use tsr_store::items::{self, ItemRow};
use tsr_store::jobs::{self, JobState};
use tsr_store::people::AliasKind;

use crate::dates::{DateContext, DateOrder, resolve};
use crate::evidence::{EvidenceError, locate, word_set_similarity};
use crate::guard::{Class, classify, looks_injected, obligation, quoted_request};
use crate::normalize::{char_slice, is_word_char, normalize, words};
use crate::people::{PersonRef, Resolver, Suppression, normalize_alias};
use crate::schema::{self, RawFact, SCHEMA_VER};

pub const MAX_SLICE_CHARS: usize = 7_200;
pub const MIN_ITEM_CHARS: usize = 30;
pub const DEDUPE_WINDOW_MS: i64 = 14 * 24 * 3600 * 1000;
pub const DEDUPE_MIN_SIMILARITY: f32 = 0.85;

pub struct Ctx<'a> {
    pub user_name: &'a str,
    pub user_aliases: &'a [&'a str],
    /// IANA name of the user's time zone, e.g. `Asia/Kolkata`.
    pub tz: &'a str,
    pub date_order: DateOrder,
    pub blocklist: &'a Blocklist,
    pub redact: RedactOptions,
    /// Model identity stored with every fact (e.g. the manifest id).
    pub model_id: &'a str,
    pub now_ms: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum Outcome {
    Done,
    AlreadyDone,
    Skipped(&'static str),
    /// Output stayed schema-invalid after the retry; nothing stored.
    Failed(&'static str),
    Preempted,
    Cancelled,
}

#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct Report {
    pub outcome: Option<Outcome>,
    pub model_calls: u32,
    pub stored: u32,
    pub merged: u32,
    /// Open loops this item reports as finished, now `proposed_done` (plan §11.3).
    pub proposed_done: u32,
    pub needs_review: u32,
    pub summary_stored: bool,
    pub people_created: u32,
    pub merge_suggestions: u32,
    pub redactions: u32,
    /// Reason → count. Reasons are fixed labels.
    pub rejected: BTreeMap<&'static str, u32>,
}

impl Report {
    fn reject(&mut self, why: &'static str) {
        *self.rejected.entry(why).or_default() += 1;
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ExtractError {
    #[error("store: {0}")]
    Store(#[from] StoreError),
    #[error("model: {0}")]
    Model(ClientError),
    #[error("item not found")]
    NotFound,
}

impl From<rusqlite::Error> for ExtractError {
    fn from(e: rusqlite::Error) -> Self {
        ExtractError::Store(e.into())
    }
}

/// Split on paragraph (then line) boundaries into slices of at most `max` characters.
pub fn slices(text: &str, max: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for para in text.split_inclusive('\n') {
        if cur.chars().count() + para.chars().count() > max && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
        if para.chars().count() > max {
            let chars: Vec<char> = para.chars().collect();
            for c in chars.chunks(max) {
                out.push(c.iter().collect());
            }
            continue;
        }
        cur.push_str(para);
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
}

fn digits(s: &str) -> Vec<String> {
    let mut v = Vec::new();
    let mut cur = String::new();
    for c in s.chars() {
        if c.is_ascii_digit() {
            cur.push(c);
        } else if !cur.is_empty() {
            v.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        v.push(cur);
    }
    v
}

fn in_source(norm_item: &str, s: &str) -> bool {
    let n = normalize(s).text;
    !n.is_empty() && norm_item.contains(&n)
}

/// A fact that passed every check before the database step.
struct Accepted {
    kind: String,
    text: String,
    text_norm: String,
    owner: OwnerSpec,
    counterparty: Option<String>,
    due_text: Option<String>,
    due: Option<(i64, &'static str, bool)>,
    status: &'static str,
    confidence: String,
    redactions: u32,
    evidence: (String, usize, usize, &'static str),
}

enum OwnerSpec {
    Me,
    Unknown,
    Name(String),
}

fn build_request(ctx: &Ctx<'_>, item: &ItemRow, src: &items::SourceRow, slice: &str, temperature: f32) -> ChatRequest {
    let p = &*EXTRACTION_V1;
    let aliases = if ctx.user_aliases.is_empty() {
        "none".to_string()
    } else {
        ctx.user_aliases.join(", ")
    };
    let system = render(p.system, &[("user_name", ctx.user_name), ("user_aliases", &aliases)]).expect("template");
    let tz = jiff::tz::db().get(ctx.tz).unwrap_or(jiff::tz::TimeZone::UTC);
    let at = jiff::Timestamp::from_millisecond(item.started_at)
        .unwrap_or(jiff::Timestamp::UNIX_EPOCH)
        .to_zoned(tz);
    let source = src
        .url
        .as_ref()
        .map(|u| u.expose().clone())
        .or_else(|| src.file_path.as_ref().map(|f| f.expose().clone()))
        .unwrap_or_else(|| format!("app:{}", src.app_exe));
    let title = src.window_title.as_ref().map(|t| t.expose().as_str()).unwrap_or("");
    let user = render(
        p.user,
        &[
            (
                "app_name",
                &escape_content(src.app_name.as_deref().unwrap_or(&src.app_exe)),
            ),
            ("title", &escape_content(title)),
            ("url_or_path", &escape_content(&source)),
            ("local_datetime", &at.strftime("%Y-%m-%d %H:%M").to_string()),
            ("weekday", &at.strftime("%A").to_string()),
            ("tz", ctx.tz),
            ("redacted_text", &escape_content(slice)),
        ],
    )
    .expect("template");
    ChatRequest {
        system: Sensitive::new(system),
        user: Sensitive::new(user),
        image_png: None,
        temperature,
        max_tokens: 600,
        json_schema: p.schema.clone(),
        stream: true,
        seed: Some(7),
    }
}

/// Run one extraction job end to end. Safe to call again after any failure or interruption.
pub fn run(
    conn: &mut Connection,
    item_id: i64,
    model: &dyn ChatModel,
    ctx: &Ctx<'_>,
    cancel: &CancelToken,
) -> Result<Report, ExtractError> {
    let mut rep = Report::default();
    let now = ctx.now_ms;
    let (job, _) = jobs::enqueue(conn, "extract", item_id, now)?;
    let row = jobs::get(conn, "extract", item_id)?.ok_or(ExtractError::NotFound)?;
    if matches!(row.state, JobState::Done | JobState::Skipped) {
        rep.outcome = Some(Outcome::AlreadyDone);
        return Ok(rep);
    }
    jobs::start(conn, job, now)?;

    // 1. authorize
    jobs::set_stage(conn, job, "authorize", now)?;
    let item = items::get_item(conn, item_id)?.ok_or(ExtractError::NotFound)?;
    if item.extraction != "pending" {
        jobs::finish(conn, job, JobState::Done, None, now)?;
        rep.outcome = Some(Outcome::AlreadyDone);
        return Ok(rep);
    }
    let src = items::get_source(conn, item.source_id)?.ok_or(ExtractError::NotFound)?;
    let title = src.window_title.as_ref().map(|t| t.expose().as_str()).unwrap_or("");
    let decision = ctx.blocklist.check(&WindowFacts {
        exe: &src.app_exe,
        exe_path: None,
        title,
        url: src.url.as_ref().map(|u| u.expose().as_str()),
        // It passed the private-window check when it was captured.
        private: PrivateState::NotPrivate,
    });
    if decision.is_blocked() {
        return skip(conn, job, item_id, "blocked_now", rep, now);
    }

    // 2. classify
    jobs::set_stage(conn, job, "classify", now)?;
    let text = item.text.expose();
    if text.trim().chars().count() < MIN_ITEM_CHARS {
        return skip(conn, job, item_id, "too_short", rep, now);
    }

    // 3–4. model + validate
    let mut outputs = Vec::new();
    for slice in slices(text, MAX_SLICE_CHARS) {
        let mut parsed = None;
        for temperature in [0.1f32, 0.0] {
            jobs::set_stage(conn, job, "model", now)?;
            rep.model_calls += 1;
            let r = model.chat(&build_request(ctx, &item, &src, &slice, temperature), cancel);
            let resp = match r {
                Ok(r) => r,
                Err(ClientError::Http(HttpError::Cancelled(reason))) => {
                    // Nothing was written for this job; it goes back to the queue (or stops).
                    jobs::finish(conn, job, JobState::Queued, None, now)?;
                    rep.outcome = Some(if reason == CancelReason::Preempted {
                        Outcome::Preempted
                    } else {
                        Outcome::Cancelled
                    });
                    return Ok(rep);
                }
                Err(e) => {
                    jobs::finish(conn, job, JobState::Queued, Some("model_error"), now)?;
                    return Err(ExtractError::Model(e));
                }
            };
            jobs::set_stage(conn, job, "validate", now)?;
            match schema::parse(resp.content.expose()) {
                Ok(o) => {
                    parsed = Some(o);
                    break;
                }
                Err(_) => rep.reject("schema_invalid_output"),
            }
        }
        match parsed {
            Some(o) => outputs.push(o),
            None => {
                items::set_extraction(conn, item_id, "failed", None)?;
                jobs::finish(conn, job, JobState::Failed, Some("schema_invalid"), now)?;
                tracing::warn!(count = rep.model_calls, reason = "schema_invalid", "extraction failed");
                rep.outcome = Some(Outcome::Failed("schema_invalid"));
                return Ok(rep);
            }
        }
    }

    // 5–6. verify, redact, suppress (no database writes yet)
    jobs::set_stage(conn, job, "verify", now)?;
    let sup = Suppression::load(conn)?;
    let norm_item = normalize(text).text;
    let self_names: Vec<String> = std::iter::once(&ctx.user_name)
        .chain(ctx.user_aliases.iter())
        .filter_map(|a| normalize_alias(AliasKind::Name, a))
        .collect();
    let tz = jiff::tz::db().get(ctx.tz).unwrap_or(jiff::tz::TimeZone::UTC);
    let reference = jiff::Timestamp::from_millisecond(item.started_at)
        .unwrap_or(jiff::Timestamp::UNIX_EPOCH)
        .to_zoned(tz);
    let dctx = DateContext::new(reference, ctx.date_order);

    let mut accepted: Vec<Accepted> = Vec::new();
    let mut summary: Option<String> = None;
    let mut people_out: Vec<(String, Option<String>)> = Vec::new();
    for out in &outputs {
        if summary.is_none() && !out.summary.trim().is_empty() {
            summary = Some(out.summary.clone());
        }
        for p in &out.people {
            people_out.push((p.name.clone(), p.handle.clone()));
        }
        for f in &out.facts {
            match verify_fact(f, text, &norm_item, &self_names, &dctx, &sup, ctx.redact, &mut rep) {
                Some(a) => accepted.push(a),
                None => continue,
            }
        }
    }

    jobs::set_stage(conn, job, "redact", now)?;
    let item_injected = text.lines().any(looks_injected);
    let summary = summary.and_then(|s| {
        let r = redact(&s, ctx.redact);
        rep.redactions += r.total();
        // A summary of an item whose facts were injections or quoted instructions would restate
        // them in the user's memory; so would one that repeats a quoted request.
        let tainted = rep.rejected.contains_key("injection") || rep.rejected.contains_key("quoted_instruction");
        if looks_injected(&r.text) || item_injected {
            rep.reject("summary_injection");
            None
        } else if tainted || quoted_request(&r.text) {
            rep.reject("summary_tainted");
            None
        } else if obligation(&r.text) {
            // Summaries describe; an instruction in one would read as a task for the user.
            rep.reject("summary_obligation");
            None
        } else if sup.mentions(&r.text) {
            rep.reject("summary_suppressed");
            None
        } else {
            Some(r.text)
        }
    });
    let people_out: Vec<(String, Option<String>)> = people_out
        .into_iter()
        .filter(|(n, h)| {
            let ok_source = in_source(&norm_item, n) || h.as_deref().is_some_and(|h| in_source(&norm_item, h));
            if !ok_source {
                rep.reject("person_not_in_source");
                return false;
            }
            if looks_injected(n) {
                rep.reject("person_injection");
                return false;
            }
            true
        })
        .map(|(n, h)| (redact(&n, ctx.redact).text, h.map(|h| redact(&h, ctx.redact).text)))
        .collect();

    // 7. persist — all or nothing.
    jobs::set_stage(conn, job, "persist", now)?;
    let tx = conn.transaction()?;
    let mut resolver = Resolver::new(&tx, &sup, ctx.user_name, ctx.user_aliases, now)?;
    for (n, h) in &people_out {
        if resolver.resolve(n, h.as_deref())? == PersonRef::Suppressed {
            rep.reject("person_suppressed");
        }
    }
    let prompt_ver = EXTRACTION_V1.ver;
    for a in &accepted {
        let (owner_id, owner_text) = match &a.owner {
            OwnerSpec::Me => (Some(resolver.self_id), None),
            OwnerSpec::Unknown => (None, None),
            OwnerSpec::Name(n) => match resolver.resolve(n, None)? {
                PersonRef::SelfPerson(id) | PersonRef::Person(id) => (Some(id), None),
                PersonRef::Unresolved => (None, Some(n.clone())),
                PersonRef::Suppressed => {
                    rep.reject("suppressed");
                    continue;
                }
            },
        };
        let (cp_id, cp_text) = match &a.counterparty {
            None => (None, None),
            Some(n) => match resolver.resolve(n, None)? {
                PersonRef::SelfPerson(id) | PersonRef::Person(id) => (Some(id), None),
                PersonRef::Unresolved => (None, Some(n.clone())),
                PersonRef::Suppressed => {
                    rep.reject("suppressed");
                    continue;
                }
            },
        };
        let ev = NewEvidence {
            item_id,
            quote: &a.evidence.0,
            start_ch: a.evidence.1 as i64,
            end_ch: a.evidence.2 as i64,
            matched: a.evidence.3,
        };
        // Completion evidence for an open loop ("sent the deck to Lena"): propose the loop done
        // (never close it) and attach the evidence there. Checked before de-duplication, which
        // would otherwise merge "sent X" into "send X" as plain evidence.
        if a.status == "open" && crate::loops::completion_signal(&a.text) {
            let c = crate::loops::Completion {
                text: &a.text,
                counterparty_id: cp_id,
                item_text: text,
                evidence: ev.clone(),
            };
            let proposed = crate::loops::propose_completions(&tx, &c, item.started_at)?;
            if !proposed.is_empty() {
                rep.proposed_done += proposed.len() as u32;
                continue;
            }
        }
        // De-duplicate against open facts of the same kind and owner.
        let cands = facts::dedupe_candidates(&tx, &a.kind, owner_id, item.started_at - DEDUPE_WINDOW_MS)?;
        let best = cands
            .iter()
            .filter(|c| a.due.is_none() || c.due_at.is_none() || c.due_at == a.due.map(|d| d.0))
            .filter(|c| {
                owner_id.is_some() || c.owner_text.as_ref().map(|t| t.expose().as_str()) == owner_text.as_deref()
            })
            .map(|c| (word_set_similarity(c.text_norm.expose(), &a.text_norm), c.id))
            .filter(|(s, _)| *s >= DEDUPE_MIN_SIMILARITY)
            .max_by(|x, y| x.0.total_cmp(&y.0).then(y.1.cmp(&x.1)));
        if let Some((_, id)) = best {
            let due = a.due.map(|(at, p, amb)| (a.due_text.as_deref(), at, p, amb));
            if facts::merge_into(&tx, id, &[ev], due, item.started_at)? {
                rep.merged += 1;
            }
            continue;
        }
        facts::insert_fact(
            &tx,
            &NewFact {
                kind: &a.kind,
                text: &a.text,
                text_norm: &a.text_norm,
                owner_id,
                owner_text: owner_text.as_deref(),
                counterparty_id: cp_id,
                counterparty_text: cp_text.as_deref(),
                due_text: a.due_text.as_deref(),
                due: a.due,
                status: a.status,
                confidence: &a.confidence,
                prompt_ver,
                model: ctx.model_id,
                schema_ver: SCHEMA_VER,
                redactions: a.redactions,
                redact_ver: REDACT_VER,
                seen_at: item.started_at,
            },
            &[ev],
        )?;
        rep.stored += 1;
        if a.status == "needs_review" {
            rep.needs_review += 1;
        }
    }
    rep.people_created = resolver.created;
    rep.merge_suggestions = resolver.suggestions;
    rep.summary_stored = summary.is_some();
    items::set_extraction(&tx, item_id, "done", summary.as_deref())?;
    jobs::finish(&tx, job, JobState::Done, None, now)?;
    tx.commit()?;
    tracing::info!(
        stored = rep.stored,
        merged = rep.merged,
        rejected = rep.rejected.values().sum::<u32>(),
        needs_review = rep.needs_review,
        "extraction finished"
    );
    rep.outcome = Some(Outcome::Done);
    Ok(rep)
}

fn skip(
    conn: &Connection,
    job: i64,
    item_id: i64,
    why: &'static str,
    mut rep: Report,
    now: i64,
) -> Result<Report, ExtractError> {
    items::set_extraction(conn, item_id, "skipped", None)?;
    jobs::finish(conn, job, JobState::Skipped, Some(why), now)?;
    rep.outcome = Some(Outcome::Skipped(why));
    Ok(rep)
}

/// Checks on one fact that need no database (stages 5–6). `None` = rejected (counted).
#[allow(clippy::too_many_arguments)]
fn verify_fact(
    f: &RawFact,
    text: &str,
    norm_item: &str,
    self_names: &[String],
    dctx: &DateContext,
    sup: &Suppression,
    ropts: RedactOptions,
    rep: &mut Report,
) -> Option<Accepted> {
    // Evidence must be in the item.
    let loc = match locate(text, &f.evidence) {
        Ok(l) => l,
        Err(EvidenceError::TooShort) => {
            rep.reject("evidence_too_short");
            return None;
        }
        Err(EvidenceError::NotInSource) => {
            rep.reject("no_evidence");
            return None;
        }
    };
    // Document content is data, not authority.
    let status = match classify(f, text, loc.start_ch, loc.end_ch, self_names) {
        Class::Injection => {
            rep.reject("injection");
            return None;
        }
        Class::Quoted => {
            rep.reject("quoted_instruction");
            return None;
        }
        Class::Ambiguous => "needs_review",
        Class::Genuine => "open",
    };
    // Owner: me, unknown, or a name that appears in the source.
    let on = normalize(&f.owner).text;
    let owner = if on == "me" || self_names.contains(&on) {
        OwnerSpec::Me
    } else if on == "unknown" || normalize_alias(AliasKind::Name, &f.owner).is_none() {
        OwnerSpec::Unknown
    } else if in_source(norm_item, &f.owner) {
        OwnerSpec::Name(f.owner.trim().to_owned())
    } else {
        rep.reject("owner_not_in_source");
        return None;
    };
    // Details that are not in the source are dropped (and counted), not kept.
    let counterparty = match &f.counterparty {
        Some(c) if normalize(c).text == "me" || self_names.contains(&normalize(c).text) => Some(c.trim().to_owned()),
        Some(c) if in_source(norm_item, c) => Some(c.trim().to_owned()),
        Some(_) => {
            rep.reject("counterparty_unsupported");
            None
        }
        None => None,
    };
    let (due_text, due) = match &f.due_text {
        Some(d) if in_source(norm_item, d) => {
            let r = resolve(d, dctx);
            (
                Some(d.trim().to_owned()),
                r.stored().map(|(at, p, amb)| (at, p.as_str(), amb)),
            )
        }
        Some(_) => {
            rep.reject("due_unsupported");
            (None, None)
        }
        None => (None, None),
    };
    // Second redaction pass over every generated field.
    let mut red = 0;
    let mut clean = |s: &str| {
        let r = redact(s, ropts);
        red += r.total();
        r.text
    };
    let text_r = clean(&f.text);
    let counterparty = counterparty.map(|c| clean(&c));
    let due_text = due_text.map(|d| clean(&d));
    let quote = char_slice(text, loc.start_ch, loc.end_ch);
    let quote_r = clean(&quote);
    if quote_r != quote {
        // The item text is already redacted, so a secret here means the span is unsafe.
        rep.reject("evidence_secret");
        return None;
    }
    let owner = match owner {
        OwnerSpec::Name(n) => OwnerSpec::Name(clean(&n)),
        o => o,
    };
    rep.redactions += red;
    // Numbers in the (redacted) claim must be in the source: no invented amounts, dates or ids.
    // Checked after redaction, so a secret copied into the text is redacted, not a rejection.
    let src_digits = digits(norm_item);
    if digits(&text_r).iter().any(|d| !src_digits.contains(d)) {
        rep.reject("unsupported_number");
        return None;
    }
    let without_placeholders = text_r.replace("[REDACTED:", " ").replace(']', " ");
    if without_placeholders.chars().filter(|c| is_word_char(*c)).count() < 3 {
        rep.reject("redacted_empty");
        return None;
    }
    // Suppressed identities never reach the facts store.
    let mentions = |s: &str| sup.mentions(s);
    if mentions(&text_r)
        || mentions(&quote)
        || matches!(&owner, OwnerSpec::Name(n) if mentions(n))
        || counterparty.as_deref().is_some_and(mentions)
    {
        rep.reject("suppressed");
        return None;
    }
    // Stored quotes are capped at 200 characters; the span is shortened with it.
    let quote_trim: String = quote_r.chars().take(200).collect();
    let end_ch = loc.start_ch + quote_trim.chars().count();
    Some(Accepted {
        kind: f.kind.clone(),
        text_norm: words(&text_r).join(" "),
        text: text_r,
        owner,
        counterparty,
        due_text,
        due,
        status,
        confidence: f.confidence.clone(),
        redactions: red,
        evidence: (quote_trim, loc.start_ch, end_ch, loc.kind.as_str()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slices_respect_paragraphs_and_limits() {
        let t = "a\n".repeat(10);
        assert_eq!(slices(&t, 6), vec!["a\na\na\n", "a\na\na\n", "a\na\na\n", "a\n"]);
        let long = "x".repeat(15);
        assert_eq!(slices(&long, 6).len(), 3);
        assert!(slices("", 10).is_empty());
    }

    #[test]
    fn digit_runs() {
        assert_eq!(digits("NW-1432 over 50k at 4:30"), vec!["1432", "50", "4", "30"]);
    }
}
