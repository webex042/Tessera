//! Retrieval evaluation (plan §18.3: recall@5/10, MRR) on the 60-question Ask set, whose gold
//! answers point at items of the extraction set (all 80 items are indexed).
//!
//! Channels compared: keyword (FTS5, any-term), vector, keyword+vector (RRF), and RRF with the
//! facts channel. **The vector channel here uses the SIMULATED hashing embedder** (no model on
//! the evaluation machine), so its numbers measure spelling overlap, not semantic search; the
//! real E5 numbers need `--model` support with the embedder files (not run). Facts for the facts
//! channel come from the oracle extraction (gold facts through the real validators).
//!
//! Behaviour checks: suppression and blocklist filters apply to every channel; a redacted value
//! is not findable; duplicate documents are reported.

use std::collections::{BTreeMap, HashMap};

use serde::Deserialize;
use serde_json::{Value, json};
use tsr_guard::blocklist::{Blocklist, RuleKind, RuleSpec, Scope, builtin_rules};
use tsr_guard::redact::Options;
use tsr_infer::cancel::CancelToken;
use tsr_pipeline::dates::DateOrder;
use tsr_pipeline::extract::{Ctx, run as extract};
use tsr_pipeline::people::Suppression;
use tsr_retrieval::embed::HashEmbedder;
use tsr_retrieval::hybrid::{Filter, Opts, search};
use tsr_retrieval::index::VectorIndex;
use tsr_retrieval::ingest::embed_item;
use tsr_store::items::Acquisition;
use tsr_store::people::{self, AliasKind};

use crate::common::*;

#[derive(Deserialize)]
struct Q {
    id: String,
    asked_at: String,
    question: String,
    gold_item_ids: Vec<String>,
    answerable: bool,
}

#[derive(Default, Clone, Copy, serde::Serialize)]
struct M {
    n: u32,
    r5: f64,
    r10: f64,
    p5: f64,
    mrr: f64,
    ndcg10: f64,
}

fn score(ranked: &[i64], gold: &[i64]) -> (f64, f64, f64, f64, f64) {
    let hit = |k: usize| ranked.iter().take(k).filter(|x| gold.contains(x)).count() as f64;
    let r5 = hit(5) / gold.len() as f64;
    let r10 = hit(10) / gold.len() as f64;
    let p5 = hit(5) / 5.0;
    let mrr = ranked
        .iter()
        .position(|x| gold.contains(x))
        .map(|p| 1.0 / (p + 1) as f64)
        .unwrap_or(0.0);
    let dcg: f64 = ranked
        .iter()
        .take(10)
        .enumerate()
        .filter(|(_, x)| gold.contains(x))
        .map(|(i, _)| 1.0 / ((i + 2) as f64).log2())
        .sum();
    let idcg: f64 = (0..gold.len().min(10)).map(|i| 1.0 / ((i + 2) as f64).log2()).sum();
    (r5, r10, p5, mrr, if idcg > 0.0 { dcg / idcg } else { 0.0 })
}

pub fn run() -> (bool, Value, String) {
    let items = load_items();
    let qs: Vec<Q> =
        serde_json::from_str(&std::fs::read_to_string(root().join("evals/ask/ask.json")).unwrap()).unwrap();
    let (_d, mut c) = db();
    let bl = Blocklist::compile(&builtin_rules()).0;
    let emb = HashEmbedder::default();
    let mut ix = VectorIndex::empty("sim-hash-v1", 384);
    let mut ids: HashMap<String, i64> = HashMap::new();
    for it in &items {
        let at = ts(&it.captured_at);
        let id = add_item(&c, &it.app, &it.title, &it.source, at, &it.text, Acquisition::Uia);
        ids.insert(it.id.clone(), id);
        embed_item(&mut c, id, &emb, Some(&mut ix), at).unwrap();
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
    assert!(ix.consistent_with(&c).unwrap(), "index matches the chunks table");

    let configs: [(&str, bool, bool, bool); 4] = [
        ("keyword", true, false, false),
        ("vector (SIMULATED embedder)", false, true, false),
        ("keyword + vector (RRF)", true, true, false),
        ("keyword + vector + facts (RRF)", true, true, true),
    ];
    let mut results: BTreeMap<&str, M> = BTreeMap::new();
    let no_filter = Filter {
        blocklist: Some(&bl),
        suppressed: None,
    };
    let mut unanswerable_hits = 0;
    let unanswerable = qs.iter().filter(|q| !q.answerable).count();
    for (name, lex, vec, facts) in configs {
        let mut m = M::default();
        for q in &qs {
            let mut o = Opts::new(ts(&q.asked_at));
            o.k = 10;
            o.use_lexical = lex;
            o.use_vector = vec;
            o.use_facts = facts;
            let (hits, _) = search(&c, Some(&ix), Some(&emb), &q.question, &no_filter, &o).unwrap();
            let ranked: Vec<i64> = hits.iter().map(|h| h.item_id).collect();
            if !q.answerable {
                if name.starts_with("keyword + vector + facts") {
                    unanswerable_hits += ranked.len().min(1);
                }
                continue;
            }
            if q.gold_item_ids.is_empty() {
                continue;
            }
            let gold: Vec<i64> = q.gold_item_ids.iter().filter_map(|g| ids.get(g).copied()).collect();
            let (r5, r10, p5, mrr, nd) = score(&ranked, &gold);
            m.n += 1;
            m.r5 += r5;
            m.r10 += r10;
            m.p5 += p5;
            m.mrr += mrr;
            m.ndcg10 += nd;
            let _ = &q.id;
        }
        let n = m.n.max(1) as f64;
        m.r5 /= n;
        m.r10 /= n;
        m.p5 /= n;
        m.mrr /= n;
        m.ndcg10 /= n;
        results.insert(name, m);
    }

    // Behaviour checks.
    let mut checks: Vec<(String, bool)> = Vec::new();
    let now = ts("2026-10-25T10:00:00+05:30");
    // Suppression: forget Kenji; no channel may return an item that mentions him.
    let kenji: Vec<i64> = people::find_by_alias(&c, AliasKind::Name, "kenji watanabe").unwrap();
    for k in &kenji {
        people::forget_person(&mut c, *k, false, now).unwrap();
    }
    let sup = Suppression::load(&c).unwrap();
    let mentions = |t: &str| sup.mentions(t);
    let filt = Filter {
        blocklist: Some(&bl),
        suppressed: Some(&mentions),
    };
    let (hits, diag) = search(
        &c,
        Some(&ix),
        Some(&emb),
        "Kenji Watanabe export fix",
        &filt,
        &Opts::new(now),
    )
    .unwrap();
    // Suppression matches the forgotten aliases (here the full name). A first-name-only mention
    // ("Kenji will fix it") is not matched — a documented limitation; the UI offers to add short
    // names as aliases before forgetting.
    let texts: Vec<String> = hits
        .iter()
        .map(|h| {
            tsr_store::items::get_item(&c, h.item_id)
                .unwrap()
                .unwrap()
                .text
                .expose()
                .to_lowercase()
        })
        .collect();
    let leaked = texts.iter().any(|t| t.contains("kenji watanabe"));
    let first_name_only = texts.iter().filter(|t| t.contains("kenji")).count();
    checks.push((
        format!(
            "suppressed person: 0 items with the forgotten name returned (filtered {}; {first_name_only} first-name-only mentions returned, known limitation)",
            diag.suppressed
        ),
        !kenji.is_empty() && !leaked,
    ));
    // Blocklist added after capture: Slack items disappear from every channel.
    let mut specs = builtin_rules();
    specs.push(RuleSpec {
        key: "user.1".into(),
        kind: RuleKind::Process,
        scope: Scope::All,
        pattern: "slack.exe".into(),
        is_regex: false,
        builtin: false,
        enabled: true,
    });
    let bl2 = Blocklist::compile(&specs).0;
    let f2 = Filter {
        blocklist: Some(&bl2),
        suppressed: None,
    };
    let (hits, diag) = search(
        &c,
        Some(&ix),
        Some(&emb),
        "design empty states copy Figma",
        &f2,
        &Opts::new(now),
    )
    .unwrap();
    let slack = hits.iter().any(|h| {
        let it = tsr_store::items::get_item(&c, h.item_id).unwrap().unwrap();
        tsr_store::items::get_source(&c, it.source_id)
            .unwrap()
            .unwrap()
            .app_exe
            .eq_ignore_ascii_case("slack.exe")
    });
    checks.push((
        format!(
            "blocklist rule added later: no Slack item returned (filtered {})",
            diag.blocked
        ),
        diag.blocked > 0 && !slack,
    ));
    // Redaction: a secret stored redacted cannot be found.
    let otp = add_item(
        &c,
        "Messages",
        "SMS",
        "app:Messages.exe",
        now,
        "Your OTP is 739104. Do not share it.",
        Acquisition::Uia,
    );
    embed_item(&mut c, otp, &emb, Some(&mut ix), now).unwrap();
    let (hits, diag) = search(
        &c,
        Some(&ix),
        Some(&emb),
        "739104",
        &Filter {
            blocklist: None,
            suppressed: None,
        },
        &Opts::new(now),
    )
    .unwrap();
    let clear = hits.iter().any(|h| {
        tsr_store::items::get_item(&c, h.item_id)
            .unwrap()
            .unwrap()
            .text
            .expose()
            .contains("739104")
    });
    let stored_otp = tsr_store::items::get_item(&c, otp)
        .unwrap()
        .unwrap()
        .text
        .expose()
        .contains("[REDACTED:otp]");
    checks.push((
        "redacted OTP value: no keyword match, no returned text contains it".into(),
        diag.lexical == 0 && !clear && stored_otp,
    ));
    // Duplicates: the same document twice is returned twice (de-duplication by source is the Ask
    // step's job, plan §10.3 step 5) and both carry provenance.
    let src = &items[0];
    let dup = add_item(&c, &src.app, &src.title, &src.source, now, &src.text, Acquisition::Uia);
    embed_item(&mut c, dup, &emb, Some(&mut ix), now).unwrap();
    let (hits, _) = search(
        &c,
        Some(&ix),
        Some(&emb),
        "empty-state illustrations Figma Onboarding",
        &no_filter,
        &Opts::new(now),
    )
    .unwrap();
    let both = hits
        .iter()
        .filter(|h| h.item_id == dup || h.item_id == ids[&src.id])
        .count();
    checks.push((
        format!("duplicate document: both copies ranked ({both}/2), each with its own item id"),
        both == 2,
    ));
    // Vector channel unavailable: keyword results still returned, flagged.
    let (hits, diag) = search(&c, None, None, "budget sheet", &no_filter, &Opts::new(now)).unwrap();
    checks.push((
        "no vector index: keyword results returned and diagnostics say vector unavailable".into(),
        !hits.is_empty() && diag.vector_unavailable,
    ));
    // Determinism.
    let a = search(
        &c,
        Some(&ix),
        Some(&emb),
        "budget sheet travel",
        &no_filter,
        &Opts::new(now),
    )
    .unwrap()
    .0;
    let b = search(
        &c,
        Some(&ix),
        Some(&emb),
        "budget sheet travel",
        &no_filter,
        &Opts::new(now),
    )
    .unwrap()
    .0;
    checks.push(("same query, same ranking".into(), a == b));

    let checks_ok = checks.iter().all(|(_, ok)| *ok);
    let mut md = String::from(
        "# Search evaluation\n\n54 answerable questions (Ask set), 80 indexed items. **Vector numbers use the SIMULATED \
         hashing embedder** (spelling overlap, not meaning); real E5 numbers are not measured on this machine.\n\n\
         | channels | recall@5 | recall@10 | precision@5 | MRR | nDCG@10 |\n|---|---|---|---|---|---|\n",
    );
    for (k, m) in &results {
        md.push_str(&format!(
            "| {k} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} |\n",
            m.r5, m.r10, m.p5, m.mrr, m.ndcg10
        ));
    }
    md.push_str(&format!(
        "\nUnanswerable questions with at least one hit (hybrid+facts): {unanswerable_hits} of {unanswerable} — retrieval always returns \
         something; answering \"not in memory\" is the Ask step's job (Stage 6).\n\nNo claim is made that hybrid beats keyword \
         in general: the table above is the only evidence, on this set, with a simulated embedder.\n\n## Behaviour checks\n\n"
    ));
    for (k, ok) in &checks {
        md.push_str(&format!("- {} {k}\n", if *ok { "PASS" } else { "FAIL" }));
    }
    let j = json!({"results": results, "checks": checks.iter().map(|(k, ok)| json!({"check": k, "pass": ok})).collect::<Vec<_>>(),
                   "unanswerable_with_hits": unanswerable_hits, "vector_embedder": "sim-hash-v1 (SIMULATED)"});
    (checks_ok, j, md)
}
