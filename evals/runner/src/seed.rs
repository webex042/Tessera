//! `tsr-eval seed --dest DIR`: fill a throwaway encrypted store with the synthetic dev-split
//! eval items, so the notch UI can be checked against real persisted data (Today, Open Loops,
//! Search, Timeline) without touching the user's profile. Run the app with `TSR_DATA_DIR=DIR`.
//!
//! Items are shifted in time so the newest lands ten minutes ago, keeping their spacing.
//! Facts come from the SIMULATED oracle (gold facts fed through the real extraction job), so
//! every stored fact still passed the real validators, guard and evidence checks.

use std::path::Path;

use serde_json::json;
use tsr_guard::blocklist::{Blocklist, builtin_rules};
use tsr_guard::redact::Options;
use tsr_infer::cancel::CancelToken;
use tsr_pipeline::dates::DateOrder;
use tsr_pipeline::extract::{Ctx, run as extract};
use tsr_store::items::Acquisition;

use crate::common::*;

/// Marker written into a seeded directory; seeding refuses any other non-empty directory.
const MARKER: &str = "SYNTHETIC-SEED";

pub fn run(dest: &Path) -> Result<String, String> {
    if dest.exists() {
        let non_empty = std::fs::read_dir(dest)
            .map_err(|e| format!("{:?}", e.kind()))?
            .next()
            .is_some();
        if non_empty && !dest.join(MARKER).exists() {
            return Err("destination is not empty and was not made by `tsr-eval seed`; refusing".into());
        }
        if dest.join(MARKER).exists() {
            std::fs::remove_dir_all(dest).map_err(|e| format!("clear old seed: {:?}", e.kind()))?;
        }
    }
    std::fs::create_dir_all(dest).map_err(|e| format!("{:?}", e.kind()))?;
    std::fs::write(dest.join(MARKER), "synthetic data from evals/extraction (dev split)\n")
        .map_err(|e| format!("{:?}", e.kind()))?;
    let mut c = tsr_store::keystore::open_store(dest).map_err(|e| e.to_string())?;

    let items: Vec<EvalItem> = load_items().into_iter().filter(|i| i.split == "dev").collect();
    let newest = items.iter().map(|i| ts(&i.captured_at)).max().unwrap_or(0);
    let shift = jiff::Timestamp::now().as_millisecond() - 10 * 60_000 - newest;
    let bl = Blocklist::compile(&builtin_rules()).0;
    let (mut n_items, mut n_facts) = (0, 0);
    for it in &items {
        let at = ts(&it.captured_at) + shift;
        let id = add_item(&c, &it.app, &it.title, &it.source, at, &it.text, Acquisition::Uia);
        n_items += 1;
        let facts: Vec<_> = it
            .expected
            .facts
            .iter()
            .map(|f| {
                json!({"kind": f.kind, "text": f.text, "owner": f.owner, "counterparty": f.counterparty,
                       "due_text": f.due_text, "evidence": f.evidence, "confidence": "high"})
            })
            .collect();
        let oracle = Scripted::one(json!({"summary": "", "facts": facts, "people": it.expected.people}).to_string());
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
        if extract(&mut c, id, &oracle, &ctx, &CancelToken::new()).is_ok() {
            n_facts += tsr_store::facts::for_item(&c, id).map(|v| v.len()).unwrap_or(0);
        }
    }
    Ok(format!(
        "seeded {n_items} synthetic items and {n_facts} facts (SIMULATED oracle extraction)"
    ))
}
