//! Draft eval (Stage 5 criteria, plan §12): the 20-scenario set in `evals/draft/draft.json`.
//!
//! - **Unflagged unsupported claims** (criterion: 0): an *independent* detector, separate from
//!   the app's checker, finds numbers, month dates, weekdays, e-mail addresses and URLs in the
//!   draft that are not in the context (thread + memory + instruction). Each must overlap a flag.
//! - Content: `must_include` present, `must_not_include` absent, placeholder when expected.
//! - Timing (real model only): time to first visible draft text and to the complete draft, p50.
//! - Without a model, a SIMULATED drafter writes replies that add invented dates, amounts, names
//!   and addresses, to test that the checker flags them (it says nothing about a model).

use std::sync::LazyLock;

use regex::Regex;
use serde::Deserialize;
use serde_json::{Value, json};
use tsr_infer::cancel::CancelToken;
use tsr_infer::client::ChatModel;

use crate::common::*;

#[derive(Debug, Deserialize)]
struct Scenario {
    id: String,
    split: String,
    app: String,
    thread_item_id: String,
    memory_item_ids: Vec<String>,
    instruction: String,
    must_include: Vec<String>,
    must_not_include: Vec<String>,
    expect_placeholder: bool,
}

static INDEPENDENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b\d[\d,.]*\b|\b(?:mon|tues|wednes|thurs|fri|satur|sun)day\b|\b(?:january|february|march|april|june|july|august|september|october|november|december|jan|feb|mar|apr|jun|jul|aug|sep|sept|oct|nov|dec)\b|[\w.+-]+@[\w-]+\.[\w.]+|https?://\S+",
    )
    .expect("regex")
});

fn norm(s: &str) -> String {
    tsr_draft::factcheck::normalise(s)
}

/// SIMULATED drafter: a plausible reply plus invented claims.
fn simulated(sc: &Scenario) -> String {
    let draft = format!(
        "Hi, thanks for the note. I'll send it by Thursday 12 Dec, and the total is $9,431. Kenji Watanabe already approved it; details at https://invented.example/x or mail finance@invented.example. Re: {}",
        sc.instruction.chars().take(60).collect::<String>()
    );
    json!({"draft": draft, "used": [], "placeholders": []}).to_string()
}

fn p50(v: &mut [u64]) -> Option<u64> {
    if v.is_empty() {
        return None;
    }
    v.sort_unstable();
    Some(v[v.len() / 2])
}

pub fn run(model: Option<&dyn ChatModel>, label: &str, split: &str) -> (bool, Value, String) {
    let path = root().join("evals/draft/draft.json");
    let all: Vec<Scenario> =
        serde_json::from_str(&std::fs::read_to_string(path).expect("draft set")).expect("draft json");
    let scenarios: Vec<&Scenario> = all.iter().filter(|s| split == "all" || s.split == split).collect();
    let items = load_items();
    let text_of = |id: &str| items.iter().find(|i| i.id == id).map(|i| i.text.clone());
    let mut rows = Vec::new();
    let (mut unflagged_total, mut valid, mut includes, mut excludes, mut placeholders) =
        (0usize, 0u32, 0u32, 0u32, 0u32);
    let (mut first, mut total): (Vec<u64>, Vec<u64>) = (Vec::new(), Vec::new());
    for sc in &scenarios {
        let Some(thread) = text_of(&sc.thread_item_id) else {
            rows.push(json!({"id": sc.id, "error": "thread item missing"}));
            continue;
        };
        let mem_texts: Vec<(i64, String)> = sc
            .memory_item_ids
            .iter()
            .enumerate()
            .filter_map(|(k, id)| text_of(id).map(|t| (k as i64 + 1, t)))
            .collect();
        let mem: Vec<(i64, &str)> = mem_texts.iter().map(|(i, t)| (*i, t.as_str())).collect();
        let input = tsr_draft::draft::DraftInput {
            app: &sc.app,
            thread: &thread,
            memory: &mem,
            instruction: &sc.instruction,
            style_examples: &[],
        };
        let scripted = Scripted::one(simulated(sc));
        let rec = Recorder::new(model.unwrap_or(&scripted));
        let r = tsr_draft::draft::run(&rec, &input, &CancelToken::new(), &mut |_| {});
        let Ok(d) = r else {
            rows.push(json!({"id": sc.id, "valid": false, "calls": rec.take()}));
            continue;
        };
        valid += 1;
        if let Some(f) = d.first_text_ms {
            first.push(f);
        }
        total.push(d.total_ms);
        let ctx = norm(&tsr_draft::draft::context_of(&input));
        let mut unflagged = Vec::new();
        for m in INDEPENDENT.find_iter(&d.text) {
            let n = norm(m.as_str());
            let n = n.trim_end_matches(['.', ',']);
            if n.is_empty() || ctx.contains(n) {
                continue;
            }
            let (s, e) = (d.text[..m.start()].chars().count(), d.text[..m.end()].chars().count());
            if !d.flags.iter().any(|f| f.start < e && f.end > s) {
                unflagged.push(m.as_str().to_string());
            }
        }
        unflagged_total += unflagged.len();
        let nd = norm(&d.text);
        let inc = sc.must_include.iter().all(|x| nd.contains(&norm(x)));
        let exc = !sc.must_not_include.iter().any(|x| nd.contains(&norm(x)));
        let ph = !sc.expect_placeholder || d.text.contains('[') || !d.placeholders.is_empty();
        includes += u32::from(inc);
        excludes += u32::from(exc);
        placeholders += u32::from(ph);
        rows.push(json!({
            "id": sc.id, "valid": true, "includes_ok": inc, "excludes_ok": exc, "placeholder_ok": ph,
            "flags": d.flags.len(), "unflagged_unsupported": unflagged,
            "first_text_ms": d.first_text_ms, "total_ms": d.total_ms,
            "draft": d.text,
        }));
    }
    let n = scenarios.len() as u32;
    let first_p50 = p50(&mut first);
    let total_p50 = p50(&mut total);
    let simulated_run = model.is_none();
    let timing_ok = simulated_run || (first_p50.is_some_and(|f| f <= 3_000) && total_p50.is_some_and(|t| t <= 8_000));
    let pass = unflagged_total == 0 && valid == n && timing_ok;
    let j = json!({
        "label": label, "split": split, "simulated": simulated_run, "scenarios": n, "valid": valid,
        "unflagged_unsupported": unflagged_total, "includes_ok": includes, "excludes_ok": excludes,
        "placeholder_ok": placeholders, "first_text_ms_p50": first_p50, "total_ms_p50": total_p50,
        "pass": pass, "rows": rows,
    });
    let md = format!(
        "# Drafts — {label}, split {split}\n\n{}\n\n| metric | value | threshold |\n|---|---|---|\n\
         | scenarios / valid drafts | {n} / {valid} | all valid |\n\
         | unsupported claims shown without a flag | {unflagged_total} | 0 |\n\
         | must-include met | {includes} / {valid} | |\n\
         | must-not-include met | {excludes} / {valid} | |\n\
         | placeholder when expected | {placeholders} / {valid} | |\n\
         | first visible text, p50 | {} | ≤ 3 000 ms |\n\
         | complete draft, p50 | {} | ≤ 8 000 ms |\n\n**{}**\n",
        if simulated_run {
            "**SIMULATED drafter:** replies with invented claims test the checker only; timings are not model timings."
        } else {
            "Real model run."
        },
        first_p50.map(|v| format!("{v} ms")).unwrap_or("—".into()),
        total_p50.map(|v| format!("{v} ms")).unwrap_or("—".into()),
        if pass { "PASS" } else { "FAIL" }
    );
    (pass, j, md)
}
