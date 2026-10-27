//! Ask eval (Stage 6 criteria, plan §10): the 60-question set in `evals/ask/ask.json`.
//!
//! Store: every eval item, with facts from the SIMULATED oracle extraction (as in `search`).
//! Retrieval as the app runs it without an embedder: keyword + facts (RRF), the question's
//! filters (time, app) applied, top 8 snippets.
//!
//! - Correct: answerable → the *verified* sentences (what the user sees as answered) contain
//!   every `answer_must_contain`; unanswerable → nothing verified. The model's own
//!   "insufficient" flag is reported but not used: code verification decides what is shown
//!   (the first real run scored 15 % with the flag, 28 % by verified content; ask/v1).
//! - Verified share: verified / shown sentences. Fabricated citations: cited numbers that do not
//!   exist. Timing (real model): first sentence and full answer, p50.
//! - Without a model, a SIMULATED oracle answers with the sentences of the gold item that hold
//!   the answer (or "insufficient"), which tests retrieval + verification, not a model.

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::{Value, json};
use tsr_guard::blocklist::{Blocklist, builtin_rules};
use tsr_guard::redact::Options;
use tsr_infer::cancel::CancelToken;
use tsr_infer::client::ChatModel;
use tsr_pipeline::dates::DateOrder;
use tsr_pipeline::extract::{Ctx, run as extract};
use tsr_retrieval::ask::{Snippet, answer, request, snippets};
use tsr_retrieval::hybrid::{Filter, Opts, search};
use tsr_store::items::Acquisition;

use crate::common::*;

#[derive(Deserialize)]
struct Q {
    id: String,
    split: String,
    asked_at: String,
    question: String,
    gold_item_ids: Vec<String>,
    answer_must_contain: Vec<String>,
    answerable: bool,
}

fn norm(s: &str) -> String {
    tsr_guard::claims::normalise(s)
}

/// SIMULATED oracle: the gold snippet's sentences that contain the answer terms.
fn oracle(q: &Q, snips: &[Snippet], gold: &[i64]) -> String {
    if !q.answerable {
        return json!({"sentences": [], "insufficient": true}).to_string();
    }
    let Some(s) = snips.iter().find(|s| gold.contains(&s.item_id)) else {
        return json!({"sentences": [], "insufficient": true}).to_string();
    };
    let text = s.text.expose();
    let sentences: Vec<Value> = text
        .split_inclusive(['.', '?', '!', '\n'])
        .map(str::trim)
        .filter(|x| !x.is_empty() && x.chars().count() <= 300)
        .filter(|x| q.answer_must_contain.iter().any(|t| norm(x).contains(&norm(t))))
        .take(3)
        .map(|x| json!({"text": x, "cites": [s.n]}))
        .collect();
    json!({"sentences": sentences, "insufficient": sentences.is_empty()}).to_string()
}

fn p50(v: &mut [u64]) -> Option<u64> {
    if v.is_empty() {
        return None;
    }
    v.sort_unstable();
    Some(v[v.len() / 2])
}

pub fn run(model: Option<&dyn ChatModel>, label: &str, split: &str) -> (bool, Value, String) {
    let items = load_items();
    let qs: Vec<Q> =
        serde_json::from_str(&std::fs::read_to_string(root().join("evals/ask/ask.json")).unwrap()).unwrap();
    let qs: Vec<&Q> = qs.iter().filter(|q| split == "all" || q.split == split).collect();
    let (_d, mut c) = db();
    let bl = Blocklist::compile(&builtin_rules()).0;
    let mut ids: HashMap<String, i64> = HashMap::new();
    for it in &items {
        let at = ts(&it.captured_at);
        let id = add_item(&c, &it.app, &it.title, &it.source, at, &it.text, Acquisition::Uia);
        ids.insert(it.id.clone(), id);
        let facts: Vec<Value> = it
            .expected
            .facts
            .iter()
            .map(|f| json!({"kind": f.kind, "text": f.text, "owner": f.owner, "counterparty": f.counterparty, "due_text": f.due_text, "evidence": f.evidence, "confidence": "high"}))
            .collect();
        let m = Scripted::one(json!({"summary": "", "facts": facts, "people": it.expected.people}).to_string());
        let ctx = Ctx {
            user_name: "Asha Verma",
            user_aliases: &["Asha"],
            tz: "Asia/Kolkata",
            date_order: DateOrder::Dmy,
            blocklist: &bl,
            redact: Options::default(),
            model_id: "oracle",
            now_ms: at,
        };
        let _ = extract(&mut c, id, &m, &ctx, &CancelToken::new());
    }
    let apps: Vec<String> = {
        let mut st = c
            .prepare("SELECT DISTINCT COALESCE(app_name, app_exe) FROM sources")
            .unwrap();
        st.query_map([], |r| r.get(0)).unwrap().flatten().collect()
    };
    let tz = jiff::tz::db().get("Asia/Kolkata").unwrap();
    let filter = Filter {
        blocklist: Some(&bl),
        suppressed: None,
    };
    let mut rows = Vec::new();
    let (mut correct, mut shown, mut verified, mut fabricated, mut invalid) = (0u32, 0u32, 0u32, 0usize, 0u32);
    let (mut first, mut total) = (Vec::new(), Vec::new());
    for q in &qs {
        let now = ts(&q.asked_at);
        let aq = tsr_pipeline::askq::parse(&q.question, now, &tz, &apps);
        let mut o = Opts::new(now);
        o.k = 40;
        o.use_vector = false; // the app has no embedder until the owner approves one
        let (hits, _) = search(&c, None, None, &q.question, &filter, &o).unwrap();
        let mut ranked = Vec::new();
        for h in &hits {
            if let Some((a, b)) = aq.range
                && !(a..b).contains(&h.started_at)
            {
                continue;
            }
            if let Some(app) = &aq.app {
                let src_app: Option<String> = c
                    .query_row(
                        "SELECT COALESCE(sources.app_name, sources.app_exe) FROM items JOIN sources ON sources.id = items.source_id WHERE items.id = ?1",
                        [h.item_id],
                        |r| r.get(0),
                    )
                    .ok();
                if src_app.as_deref() != Some(app.as_str()) {
                    continue;
                }
            }
            ranked.push(h.item_id);
        }
        let snips = snippets(&c, &ranked, &q.question).unwrap();
        let gold: Vec<i64> = q.gold_item_ids.iter().filter_map(|g| ids.get(g).copied()).collect();
        let local = jiff::Timestamp::from_millisecond(now).unwrap().to_zoned(tz.clone());
        let label_of = |ms: i64| {
            jiff::Timestamp::from_millisecond(ms)
                .map(|t| t.to_zoned(tz.clone()).strftime("%a %-d %b %H:%M").to_string())
                .unwrap_or_default()
        };
        let req = request(
            &q.question,
            &snips,
            &local.strftime("%Y-%m-%d").to_string(),
            &local.strftime("%A").to_string(),
            &label_of,
        );
        let scripted = Scripted::one(oracle(q, &snips, &gold));
        let rec = Recorder::new(model.unwrap_or(&scripted));
        let Ok(a) = answer(&rec, &req, &snips, &CancelToken::new(), &mut |_| {}) else {
            invalid += 1;
            rows.push(json!({"id": q.id, "valid": false, "calls": rec.take()}));
            continue;
        };
        if let Some(f) = a.first_sentence_ms {
            first.push(f);
        }
        total.push(a.total_ms);
        let ok_text = norm(
            &a.sentences
                .iter()
                .filter(|s| s.verified)
                .map(|s| s.text.as_str())
                .collect::<Vec<_>>()
                .join(" "),
        );
        let is_correct = if q.answerable {
            !ok_text.is_empty() && q.answer_must_contain.iter().all(|t| ok_text.contains(&norm(t)))
        } else {
            a.sentences.iter().all(|s| !s.verified)
        };
        correct += u32::from(is_correct);
        shown += a.sentences.len() as u32;
        verified += a.sentences.iter().filter(|s| s.verified).count() as u32;
        fabricated += a.sentences.iter().map(|s| s.fabricated).sum::<usize>();
        rows.push(json!({
            "id": q.id, "correct": is_correct, "insufficient": a.insufficient,
            "gold_retrieved": snips.iter().any(|s| gold.contains(&s.item_id)),
            "filters": {"range": aq.range.is_some(), "app": aq.app, "kind": aq.kind},
            "sentences": a.sentences, "first_sentence_ms": a.first_sentence_ms, "total_ms": a.total_ms,
        }));
    }
    let n = qs.len() as u32;
    let acc = f64::from(correct) / f64::from(n.max(1));
    let vshare = if shown > 0 {
        f64::from(verified) / f64::from(shown)
    } else {
        1.0
    };
    let (f50, t50) = (p50(&mut first), p50(&mut total));
    let simulated = model.is_none();
    let timing_ok = simulated || (f50.is_some_and(|v| v <= 2_500) && t50.is_some_and(|v| v <= 10_000));
    let pass = acc >= 0.80 && vshare >= 0.90 && fabricated == 0 && timing_ok;
    let j = json!({
        "label": label, "split": split, "simulated": simulated, "questions": n, "invalid": invalid,
        "correct": correct, "accuracy": acc, "shown_sentences": shown, "verified_sentences": verified,
        "verified_share": vshare, "fabricated_citations": fabricated,
        "first_sentence_ms_p50": f50, "total_ms_p50": t50, "pass": pass, "rows": rows,
    });
    let md = format!(
        "# Ask — {label}, split {split}\n\n{}\nRetrieval: keyword + facts (no embedder), filters from the question; facts from SIMULATED oracle extraction.\n\n\
         | metric | value | threshold |\n|---|---|---|\n\
         | questions / invalid answers | {n} / {invalid} | |\n\
         | answered correctly | {correct} ({:.1} %) | ≥ 80 % |\n\
         | shown sentences verified | {verified} / {shown} ({:.1} %) | ≥ 90 % |\n\
         | fabricated citations | {fabricated} | 0 |\n\
         | first sentence, p50 | {} | ≤ 2 500 ms |\n\
         | full answer, p50 | {} | ≤ 10 000 ms |\n\n**{}**\n",
        if simulated {
            "**SIMULATED oracle answers:** tests retrieval and citation checks, not a model; timings are not model timings.\n"
        } else {
            "Real model run.\n"
        },
        acc * 100.0,
        vshare * 100.0,
        f50.map(|v| format!("{v} ms")).unwrap_or("—".into()),
        t50.map(|v| format!("{v} ms")).unwrap_or("—".into()),
        if pass { "PASS" } else { "FAIL" }
    );
    (pass, j, md)
}
