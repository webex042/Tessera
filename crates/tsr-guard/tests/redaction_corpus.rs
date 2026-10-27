//! Stage 1 exit criteria for redaction (plan §6.3, stages.md Stage 1):
//! - 100 % of seeded secrets redacted (tests/fixtures/redaction/secrets.jsonl);
//! - false-positive rate ≤ 2 % on clean text (tests/fixtures/redaction/clean.txt plus the 80
//!   synthetic screen texts in evals/extraction/items).
//!
//! All corpus values are synthetic (tools/gen_redaction_corpus.py).

use std::collections::BTreeMap;
use std::path::PathBuf;

use tsr_guard::redact::{Options, redact};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn squash(s: &str) -> String {
    s.chars().filter(|c| !matches!(c, ' ' | '-')).collect()
}

#[test]
fn every_seeded_secret_is_redacted() {
    let text = std::fs::read_to_string(root().join("tests/fixtures/redaction/secrets.jsonl")).unwrap();
    let mut per_kind: BTreeMap<String, (u32, u32)> = BTreeMap::new();
    let mut misses = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        let id = v["id"].as_str().unwrap();
        let kind = v["kind"].as_str().unwrap().to_string();
        let out = redact(v["text"].as_str().unwrap(), Options::default()).text;
        let flat = squash(&out);
        let mut ok = true;
        for s in v["secrets"].as_array().unwrap() {
            let s = s.as_str().unwrap();
            // Present as written, or with separators removed (a partial redaction of a grouped
            // number would leave the remaining groups readable).
            if out.contains(s) || flat.contains(&squash(s)) {
                ok = false;
            }
        }
        let e = per_kind.entry(kind).or_default();
        e.1 += 1;
        if ok {
            e.0 += 1;
        } else {
            misses.push(format!("{id}: {out}"));
        }
    }
    for (k, (hit, n)) in &per_kind {
        println!("recall {k:12} {hit}/{n}");
    }
    let total: u32 = per_kind.values().map(|v| v.1).sum();
    assert!(total >= 250, "corpus too small: {total}");
    assert!(
        misses.is_empty(),
        "{} of {total} examples leaked a secret:\n{}",
        misses.len(),
        misses.join("\n")
    );
}

#[test]
fn false_positive_rate_on_clean_text_is_at_most_two_percent() {
    let mut docs: Vec<(String, String)> = std::fs::read_to_string(root().join("tests/fixtures/redaction/clean.txt"))
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .enumerate()
        .map(|(i, l)| (format!("clean:{}", i + 1), l.to_string()))
        .collect();
    for f in std::fs::read_dir(root().join("evals/extraction/items")).unwrap() {
        let items: Vec<serde_json::Value> =
            serde_json::from_str(&std::fs::read_to_string(f.unwrap().path()).unwrap()).unwrap();
        for it in items {
            docs.push((
                it["id"].as_str().unwrap().to_string(),
                it["text"].as_str().unwrap().to_string(),
            ));
        }
    }
    let mut fps = Vec::new();
    for (id, text) in &docs {
        let r = redact(text, Options::default());
        if r.total() > 0 {
            fps.push(format!("{id} {:?}: {}", r.counts, r.text));
        }
    }
    let rate = fps.len() as f64 / docs.len() as f64;
    println!("false positives: {}/{} = {:.2} %", fps.len(), docs.len(), rate * 100.0);
    for f in &fps {
        println!("  FP {f}");
    }
    assert!(docs.len() >= 200, "clean corpus too small: {}", docs.len());
    assert!(rate <= 0.02, "false-positive rate {:.2} % > 2 %", rate * 100.0);
}
