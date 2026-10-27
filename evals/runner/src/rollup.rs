//! Rollups with a model (Stage 4): replay synthetic items over a week and roll them up.
//!
//! - Items: the dev-split extraction items, spread over Monday–Saturday (two per active hour).
//!   Their stored summary is the first 200 characters of the item text, standing in for an
//!   extraction summary (SIMULATED input; the rollup code and model are real).
//! - Measures: an hour summary for every active hour and none for idle hours; every highlight id
//!   resolves to an item; how many summaries the model wrote vs the code-only fallback after two
//!   invalid outputs; latency per model call.
//! - Pass: every active hour summarised, all ids valid, ≤ 10 % fallbacks among model attempts.

use std::collections::BTreeSet;
use std::time::Instant;

use serde_json::{Value, json};
use tsr_guard::redact::Options;
use tsr_infer::cancel::CancelToken;
use tsr_infer::client::ChatModel;
use tsr_pipeline::rollup::{Generated, Level, RollupCtx, run_all_due};
use tsr_store::items::Acquisition;
use tsr_store::summaries;

use crate::common::*;

pub fn run(model: Option<&dyn ChatModel>, label: &str) -> (bool, Value, String) {
    let (_d, mut c) = db();
    let tz = jiff::tz::db().get("Asia/Kolkata").expect("tz");
    let items: Vec<EvalItem> = load_items().into_iter().filter(|i| i.split == "dev").collect();
    let hours = [9, 10, 11, 14, 15];
    let mut active: BTreeSet<i64> = BTreeSet::new();
    let mut k = 0;
    'days: for day in 5..=10 {
        for h in hours {
            for m in [5, 40] {
                let Some(it) = items.get(k) else { break 'days };
                k += 1;
                let at = ts(&format!("2026-10-{day:02}T{h:02}:{m:02}:00+05:30"));
                let id = add_item(&c, &it.app, &it.title, &it.source, at, &it.text, Acquisition::Uia);
                let summary: String = it.text.chars().take(200).collect();
                tsr_store::items::set_extraction(&c, id, "done", Some(&summary)).expect("summary");
                active.insert(ts(&format!("2026-10-{day:02}T{h:02}:00:00+05:30")));
            }
        }
    }
    let now = ts("2026-10-13T12:00:00+05:30");
    let rec = model.map(Recorder::new);
    let ctx = RollupCtx {
        tz: &tz,
        redact: Options::default(),
        model_id: label,
        now_ms: now,
    };
    let t = Instant::now();
    let done = run_all_due(
        &mut c,
        rec.as_ref().map(|r| r as &dyn ChatModel),
        &ctx,
        0,
        &CancelToken::new(),
    )
    .expect("rollups");
    let secs = t.elapsed().as_secs_f64();

    let (mut by_model, mut fallback, mut other) = (0u32, 0u32, 0u32);
    for (_, g) in &done {
        match g {
            Generated::Model { .. } => by_model += 1,
            Generated::Extractive { model_failed: true, .. } => fallback += 1,
            _ => other += 1,
        }
    }
    let span = (ts("2026-10-01T00:00:00+05:30"), now);
    let got: BTreeSet<i64> = summaries::in_range(&c, "hour", span.0, span.1)
        .unwrap()
        .iter()
        .map(|s| s.period_start)
        .collect();
    let mut bad_ids = 0;
    let mut highlights = 0;
    for level in Level::ALL {
        for s in summaries::in_range(&c, level.as_str(), span.0, span.1).unwrap() {
            for h in s.highlights.expose() {
                highlights += 1;
                if tsr_store::items::existing(&c, &h.item_ids).unwrap().len() != h.item_ids.len()
                    || h.item_ids.is_empty()
                {
                    bad_ids += 1;
                }
            }
        }
    }
    let attempts = by_model + fallback;
    let fallback_rate = if attempts > 0 {
        f64::from(fallback) / f64::from(attempts)
    } else {
        0.0
    };
    let calls = rec.as_ref().map(|r| r.take()).unwrap_or_default();
    let mean_ms = if calls.is_empty() {
        0.0
    } else {
        calls.iter().filter_map(|c| c["ms"].as_f64()).sum::<f64>() / calls.len() as f64
    };
    let finish_length = calls.iter().filter(|c| c["finish"] == "length").count();
    let pass = got == active && bad_ids == 0 && (model.is_none() || fallback_rate <= 0.10);
    let failed_examples: Vec<&Value> = calls.iter().filter(|c| c["finish"] != "stop").take(5).collect();
    let j = json!({
        "label": label, "items": k, "active_hours": active.len(), "hour_summaries": got.len(),
        "exact_hours": got == active, "periods": done.len(), "by_model": by_model, "fallback": fallback,
        "other": other, "fallback_rate": fallback_rate, "highlights": highlights, "bad_highlight_ids": bad_ids,
        "model_calls": calls.len(), "mean_call_ms": mean_ms, "finish_length": finish_length, "seconds": secs,
        "non_stop_examples": failed_examples, "pass": pass,
    });
    let md = format!(
        "# Rollups — {label}\n\n{k} synthetic items over 6 days; inputs SIMULATED (item summary = first 200 chars).\n\n\
         | metric | value | threshold |\n|---|---|---|\n\
         | active hours / hour summaries | {} / {} (exact: {}) | equal |\n\
         | periods rolled up | {} | |\n\
         | written by the model | {by_model} | |\n\
         | fallback after two invalid outputs | {fallback} ({:.1} %) | ≤ 10 % |\n\
         | highlights / with invalid ids | {highlights} / {bad_ids} | 0 invalid |\n\
         | model calls / mean ms / finish=length | {} / {:.0} / {finish_length} | |\n\
         | wall time | {:.0} s | |\n\n**{}**\n",
        active.len(),
        got.len(),
        got == active,
        done.len(),
        fallback_rate * 100.0,
        calls.len(),
        mean_ms,
        secs,
        if pass { "PASS" } else { "FAIL" }
    );
    (pass, j, md)
}
