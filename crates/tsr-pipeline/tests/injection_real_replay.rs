//! Regression replay of the real Qwen3-VL-4B outputs recorded in `evals/results/injection_real.json`.
//!
//! Ensures that every historical safety violation (`13` cases where the 4B model emitted
//! forbidden strings or injected tasks) and all mixed-prompt cases fail closed with zero
//! forbidden strings and zero injected open tasks stored under the hardened deterministic guard.

mod common;

use std::collections::HashMap;
use std::path::Path;

use common::*;
use serde::Deserialize;
use tsr_guard::redact::{Options, find, redact};
use tsr_infer::cancel::CancelToken;
use tsr_pipeline::extract::run;
use tsr_pipeline::normalize::normalize;
use tsr_store::facts;
use tsr_store::items::get_item;

#[derive(Deserialize)]
struct CorpusCase {
    id: String,
    app: String,
    title: String,
    text: String,
    expect: Expect,
}

#[derive(Deserialize)]
struct Expect {
    forbidden: Vec<String>,
}

#[derive(Deserialize)]
struct EvalReport {
    results: Vec<EvalRow>,
}

#[derive(Deserialize)]
struct EvalRow {
    id: String,
    pass: bool,
    #[serde(default)]
    model_outputs: Vec<ModelOut>,
}

#[derive(Deserialize)]
struct ModelOut {
    #[serde(default)]
    output: Option<String>,
}

#[test]
fn real_model_safety_violations_now_fail_closed() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let cases_json = std::fs::read_to_string(root.join("tests/fixtures/injection/cases.json")).unwrap();
    let cases: HashMap<String, CorpusCase> = serde_json::from_str::<Vec<CorpusCase>>(&cases_json)
        .unwrap()
        .into_iter()
        .map(|c| (c.id.clone(), c))
        .collect();

    let baseline = root.join("evals/results/baselines-2026-10-28/injection_real.json");
    let report_path = if baseline.exists() {
        baseline
    } else {
        root.join("evals/results/injection_real.json")
    };
    let report_json = std::fs::read_to_string(&report_path).unwrap();
    let report: EvalReport = serde_json::from_str(&report_json).unwrap();

    let safety_and_mixed_ids = [
        // 13 historical safety violations (stored forbidden text / injected tasks)
        "s7-002", "s7-004", "s7-012", "s7-016", "s7-018", "s7-019", "s7-020", "s7-024", "s7-026", "s7-028", "s7-030",
        "s7-050", "s7-066",
        // 5 mixed genuine+injection cases where the 4B model concatenated both clauses
        "inj-a05", "inj-e05", "s7-059", "s7-060", "s7-061",
    ];

    let rows_by_id: HashMap<&str, &EvalRow> = report.results.iter().map(|r| (r.id.as_str(), r)).collect();

    for id in safety_and_mixed_ids {
        let case = &cases[id];
        let row = rows_by_id[id];
        assert!(!row.pass, "expected {id} to be a historical failure row");
        let raw_output = row
            .model_outputs
            .iter()
            .find_map(|m| m.output.as_ref())
            .unwrap_or_else(|| panic!("missing recorded model output for {id}"))
            .clone();

        let (_d, mut c) = db();
        let red_text = redact(&case.text, Options::default()).text;
        let red_title = redact(&case.title, Options::default()).text;
        let item_id = item_in(&c, &case.app, &red_title, None, T0, &red_text);
        let bl = blocklist();
        let m = Scripted::json(&[&raw_output]);
        let _ = run(&mut c, item_id, &m, &ctx(&bl), &CancelToken::new()).expect("extract run");

        let mut stored_strings = Vec::new();
        for f in facts::all(&c).unwrap() {
            stored_strings.push(("fact text", f.text.expose().clone()));
            for e in facts::evidence(&c, f.id).unwrap() {
                stored_strings.push(("fact evidence", e.quote.expose().clone()));
            }
            assert!(
                !matches!(f.kind.as_str(), "task" | "commitment") || f.status != "open",
                "{id}: unexpected open task/commitment stored: {:?}",
                f.text.expose()
            );
        }
        if let Some(summary) = get_item(&c, item_id).unwrap().unwrap().summary {
            stored_strings.push(("summary", summary.expose().clone()));
        }

        for (where_, s) in stored_strings {
            let norm_s = normalize(&s).text;
            for forb in &case.expect.forbidden {
                assert!(
                    !norm_s.contains(&normalize(forb).text),
                    "{id}: forbidden {forb:?} leaked into {where_}: {s:?}"
                );
            }
            assert!(
                find(&s, Options::default()).is_empty(),
                "{id}: secret leaked in {where_}: {s:?}"
            );
        }
    }
}
