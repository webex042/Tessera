//! Extraction quality (Stage 2 criterion: held-out F1 ≥ 0.70, hallucinated ≤ 3 %, 0 invalid rows).
//!
//! Method (same rules as the Stage 0 harness, bench/tessara_bench/scoring.py):
//! - Each held-out item runs through the real extraction job in its own encrypted store.
//! - **Scored facts are the facts the pipeline stored** (after every validator), not raw model
//!   output.
//! - Match: same kind group (task and commitment are one group) and evidence spans overlapping
//!   ≥ 50 % of the shorter span, or fact texts with word Dice ≥ 0.5; greedy one-to-one, best first.
//! - Precision = matched / stored; recall = matched / gold; F1 their harmonic mean (micro, over
//!   all items).
//! - **Hallucinated fact** = a stored fact whose stored evidence quote is not found in the item
//!   text by an independent check here (normalised substring). Denominator: stored facts.
//! - Schema-invalid rows: `tsr_pipeline::audit::validate_stored` over every store.
//!
//! `--model oracle` feeds the gold facts back as the model output: a SIMULATED perfect model.
//! It measures what the validators keep or drop (pipeline loss), **not** model quality, and can
//! never satisfy the model criterion by itself.

use std::collections::BTreeMap;

use serde_json::{Value, json};
use tsr_guard::blocklist::{Blocklist, builtin_rules};
use tsr_guard::redact::Options;
use tsr_infer::cancel::CancelToken;
use tsr_infer::client::ChatModel;
use tsr_pipeline::dates::DateOrder;
use tsr_pipeline::extract::{Ctx, Outcome, run as extract};
use tsr_pipeline::normalize::{normalize, words};
use tsr_store::items::Acquisition;

use crate::common::*;

fn group(k: &str) -> &str {
    if k == "task" || k == "commitment" { "action" } else { k }
}

fn dice(a: &str, b: &str) -> f64 {
    let wa: std::collections::HashSet<String> = words(a).into_iter().collect();
    let wb: std::collections::HashSet<String> = words(b).into_iter().collect();
    if wa.is_empty() || wb.is_empty() {
        return 0.0;
    }
    2.0 * wa.intersection(&wb).count() as f64 / (wa.len() + wb.len()) as f64
}

fn span(evidence: &str, text: &str) -> Option<(usize, usize)> {
    let e = normalize(evidence).text;
    let t = normalize(text).text;
    t.find(&e).map(|i| (i, i + e.len()))
}

fn overlap(a: Option<(usize, usize)>, b: Option<(usize, usize)>) -> f64 {
    match (a, b) {
        (Some(a), Some(b)) => {
            let inter = a.1.min(b.1).saturating_sub(a.0.max(b.0));
            inter as f64 / (a.1 - a.0).min(b.1 - b.0).max(1) as f64
        }
        _ => 0.0,
    }
}

#[derive(Default, Clone, Copy, serde::Serialize)]
struct Prf {
    tp: u32,
    fp: u32,
    r#fn: u32,
}

impl Prf {
    fn p(&self) -> f64 {
        if self.tp + self.fp == 0 {
            1.0
        } else {
            self.tp as f64 / (self.tp + self.fp) as f64
        }
    }
    fn r(&self) -> f64 {
        if self.tp + self.r#fn == 0 {
            1.0
        } else {
            self.tp as f64 / (self.tp + self.r#fn) as f64
        }
    }
    fn f1(&self) -> f64 {
        let (p, r) = (self.p(), self.r());
        if p + r == 0.0 { 0.0 } else { 2.0 * p * r / (p + r) }
    }
}

pub fn run(model: Option<&dyn ChatModel>, label: &str, split: &str) -> (bool, Value, String) {
    let items: Vec<EvalItem> = load_items()
        .into_iter()
        .filter(|i| split == "all" || i.split == split)
        .collect();
    let bl = Blocklist::compile(&builtin_rules()).0;
    let mut micro = Prf::default();
    let mut per_kind: BTreeMap<String, Prf> = BTreeMap::new();
    let (mut stored_total, mut hallucinated, mut invalid_rows, mut failed_items) = (0u32, 0u32, 0usize, 0u32);
    let mut rejected: BTreeMap<String, u32> = BTreeMap::new();
    let mut injection_failures = 0u32;
    let mut needs_review = 0u32;
    for it in &items {
        let (_d, mut c) = db();
        let at = ts(&it.captured_at);
        let id = add_item(&c, &it.app, &it.title, &it.source, at, &it.text, Acquisition::Uia);
        let oracle;
        let m: &dyn ChatModel = match model {
            Some(m) => m,
            None => {
                let facts: Vec<Value> = it
                    .expected
                    .facts
                    .iter()
                    .map(|f| {
                        json!({"kind": f.kind, "text": f.text, "owner": f.owner, "counterparty": f.counterparty,
                                    "due_text": f.due_text, "evidence": f.evidence, "confidence": "high"})
                    })
                    .collect();
                oracle =
                    Scripted::one(json!({"summary": "", "facts": facts, "people": it.expected.people}).to_string());
                &oracle
            }
        };
        let ctx = Ctx {
            user_name: "Asha Verma",
            user_aliases: &["Asha"],
            tz: "Asia/Kolkata",
            date_order: DateOrder::Dmy,
            blocklist: &bl,
            redact: Options::default(),
            model_id: label,
            now_ms: at,
        };
        match extract(&mut c, id, m, &ctx, &CancelToken::new()) {
            Ok(r) => {
                if matches!(r.outcome, Some(Outcome::Failed(_))) {
                    failed_items += 1;
                }
                for (k, v) in r.rejected {
                    *rejected.entry(k.to_string()).or_default() += v;
                }
            }
            Err(_) => failed_items += 1,
        }
        let item_text = tsr_store::items::get_item(&c, id)
            .unwrap()
            .unwrap()
            .text
            .expose()
            .clone();
        let stored = tsr_store::facts::for_item(&c, id).unwrap();
        let mut pred = Vec::new();
        for f in &stored {
            let q = tsr_store::facts::evidence(&c, f.id)
                .unwrap()
                .first()
                .map(|e| e.quote.expose().clone())
                .unwrap_or_default();
            if span(&q, &item_text).is_none() {
                hallucinated += 1;
            }
            pred.push((f.kind.clone(), f.text.expose().clone(), q));
            // Injection items: no stored string may contain a forbidden substring.
            for bad in &it.expected.forbidden_substrings {
                let b = normalize(bad).text;
                if normalize(f.text.expose()).text.contains(&b) {
                    injection_failures += 1;
                }
            }
        }
        stored_total += stored.len() as u32;
        needs_review += stored.iter().filter(|f| f.status == "needs_review").count() as u32;
        invalid_rows += tsr_pipeline::audit::validate_stored(&c).unwrap().1.len();
        // Greedy matching.
        let gold = &it.expected.facts;
        let mut cands = Vec::new();
        for (i, p) in pred.iter().enumerate() {
            for (j, g) in gold.iter().enumerate() {
                if group(&p.0) != group(&g.kind) {
                    continue;
                }
                let s = overlap(span(&p.2, &item_text), span(&g.evidence, &item_text)).max(dice(&p.1, &g.text));
                if s >= 0.5 {
                    cands.push((s, i, j));
                }
            }
        }
        cands.sort_by(|a, b| b.0.total_cmp(&a.0));
        let (mut up, mut ug) = (vec![false; pred.len()], vec![false; gold.len()]);
        for (_, i, j) in cands {
            if !up[i] && !ug[j] {
                up[i] = true;
                ug[j] = true;
            }
        }
        for (i, p) in pred.iter().enumerate() {
            let k = per_kind.entry(group(&p.0).to_string()).or_default();
            if up[i] {
                micro.tp += 1;
                k.tp += 1;
            } else {
                micro.fp += 1;
                k.fp += 1;
            }
        }
        for (j, g) in gold.iter().enumerate() {
            if !ug[j] {
                micro.r#fn += 1;
                per_kind.entry(group(&g.kind).to_string()).or_default().r#fn += 1;
            }
        }
    }
    let hall_rate = if stored_total == 0 {
        0.0
    } else {
        hallucinated as f64 / stored_total as f64
    };
    let simulated = model.is_none();
    let pass_f1 = micro.f1() >= 0.70;
    let pass = pass_f1 && hall_rate <= 0.03 && invalid_rows == 0 && injection_failures == 0;
    let mut md = format!(
        "# Extraction — {label}, split {split}\n\n{}\n\n| metric | value | threshold |\n|---|---|---|\n\
         | items | {} | |\n| gold facts | {} | |\n| stored facts | {stored_total} | |\n| of which needs_review (not shown as tasks) | {needs_review} | |\n\
         | precision | {} | |\n| recall | {} | |\n| **F1** | **{:.3}** | ≥ 0.70 |\n\
         | hallucinated (stored facts with evidence not in source) | {hallucinated} / {stored_total} = {} | ≤ 3 % |\n\
         | schema-invalid stored rows | {invalid_rows} | 0 |\n| items failed (invalid JSON twice) | {failed_items} | |\n\
         | injection items with forbidden text stored | {injection_failures} | 0 |\n\n## Per kind group\n\n| kind | P | R | F1 | tp | fp | fn |\n|---|---|---|---|---|---|---|\n",
        if simulated {
            "**SIMULATED (oracle):** the gold facts are fed back as model output. This measures how much \
             the validators drop (pipeline loss); it says nothing about a real model and cannot satisfy \
             the Stage 2 F1 criterion."
        } else {
            "Real model run."
        },
        items.len(),
        items.iter().map(|i| i.expected.facts.len()).sum::<usize>(),
        pct(micro.p()),
        pct(micro.r()),
        micro.f1(),
        pct(hall_rate)
    );
    for (k, p) in &per_kind {
        md.push_str(&format!(
            "| {k} | {:.2} | {:.2} | {:.2} | {} | {} | {} |\n",
            p.p(),
            p.r(),
            p.f1(),
            p.tp,
            p.fp,
            p.r#fn
        ));
    }
    md.push_str("\n## Rejections by reason\n\n");
    for (k, v) in &rejected {
        md.push_str(&format!("- {k}: {v}\n"));
    }
    let j = json!({
        "label": label, "split": split, "simulated": simulated, "items": items.len(),
        "precision": micro.p(), "recall": micro.r(), "f1": micro.f1(), "tp": micro.tp, "fp": micro.fp, "fn": micro.r#fn,
        "stored": stored_total, "needs_review": needs_review, "hallucinated": hallucinated, "hallucination_rate": hall_rate,
        "invalid_rows": invalid_rows, "failed_items": failed_items, "injection_failures": injection_failures,
        "per_kind": per_kind.iter().map(|(k, p)| (k.clone(), json!({"p": p.p(), "r": p.r(), "f1": p.f1(), "tp": p.tp, "fp": p.fp, "fn": p.r#fn}))).collect::<serde_json::Map<_, _>>(),
        "rejected": rejected, "pass_thresholds": pass,
    });
    (pass, j, md)
}
