//! `tsr-eval`: Stage 2 evaluation runners over the real pipeline code (plan §18.3).
//!
//!     tsr-eval injection [--model DIR]          prompt-injection corpus (default: SIMULATED compromised model)
//!     tsr-eval extraction --model oracle|DIR [--split heldout|dev|all] [--profile P]
//!     tsr-eval search                           retrieval: keyword, vector (SIMULATED embedder), hybrid
//!     tsr-eval rollup [--model DIR]             week replay through rollups (no model: code-only)
//!     tsr-eval draft [--model DIR] [--split S] [--profile P]  20 draft scenarios (no model: SIMULATED drafter)
//!     tsr-eval ask [--model DIR] [--split S] [--profile P]    60 Ask questions (no model: SIMULATED oracle)
//!     tsr-eval backlog [--profile P]            2-hour synthetic workload through the real scheduler
//!     tsr-eval scorecard                        two-profile summary over immutable measured results
//!     tsr-eval all [--profile P]                everything that needs no model (CI)
//!     tsr-eval seed --dest DIR                  synthetic store for manual UI checks (TSR_DATA_DIR=DIR)
//!
//! `--profile P`: `standard` (default, RTX 4050 product target) or `local-igpu-beta`
//! (Qwen3-VL-4B on AMD integrated GPU). `--model DIR` uses real model files in DIR.

mod ask;
mod backlog;
mod common;
mod draft;
mod extraction;
mod injection;
mod profile;
mod rollup;
mod search;
mod seed;

use std::path::PathBuf;

use crate::profile::EvalProfile;

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn finish(name: &str, (pass, json, md): (bool, serde_json::Value, String)) -> bool {
    let p = common::write_report(name, &json, &md);
    println!(
        "{}\n-> {}",
        md.lines().take(3).collect::<Vec<_>>().join("\n"),
        p.display()
    );
    pass
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().cloned().unwrap_or_default();
    let profile = match arg(&args, "--profile") {
        None => EvalProfile::Standard,
        Some(p) => match EvalProfile::parse(&p) {
            Some(ep) => ep,
            None => {
                eprintln!("tsr-eval: unknown --profile {p:?} (expected standard or local-igpu-beta)");
                std::process::exit(2);
            }
        },
    };
    let model_arg = arg(&args, "--model");
    // `--ids a,b,c`: run only these cases/items (diagnosis); reports are named `*_subset`.
    let ids: Option<Vec<String>> = arg(&args, "--ids").map(|s| s.split(',').map(str::to_owned).collect());
    let needs_llm = matches!(cmd.as_str(), "injection" | "extraction" | "draft" | "ask" | "rollup");
    let real = if needs_llm {
        match model_arg.as_deref() {
            None | Some("oracle") | Some("simulated") => None,
            Some(dir) => match common::real_model(&PathBuf::from(dir)) {
                Ok(m) => Some(m),
                Err(e) => {
                    eprintln!("tsr-eval: cannot start a real model: {e}");
                    std::process::exit(2);
                }
            },
        }
    } else {
        None
    };
    let embedder = if matches!(cmd.as_str(), "search" | "ask") {
        match model_arg.as_deref() {
            None | Some("oracle") | Some("simulated") => None,
            Some(dir) => match common::real_embedder(&PathBuf::from(dir)) {
                Ok(e) => e,
                Err(e) => {
                    eprintln!("tsr-eval: cannot load embedder: {e}");
                    std::process::exit(2);
                }
            },
        }
    } else {
        None
    };
    let emb_dyn = embedder.as_ref().map(|e| e as &dyn tsr_retrieval::embed::Embedder);
    let label = real
        .as_ref()
        .map(|m| m.label.clone())
        .unwrap_or_else(|| "simulated".into());
    let real_dyn = real.as_ref().map(|m| m as &dyn tsr_infer::client::ChatModel);
    let ok = match cmd.as_str() {
        "injection" => {
            let corpus_arg = arg(&args, "--corpus");
            let corpus_path = corpus_arg
                .as_ref()
                .map(|c| {
                    let p = std::path::PathBuf::from(c);
                    if p.is_absolute() { p } else { common::root().join(p) }
                })
                .unwrap_or_else(|| common::root().join("tests/fixtures/injection/cases.json"));
            let suffix = corpus_arg
                .as_ref()
                .and_then(|c| std::path::Path::new(c).file_stem().and_then(|s| s.to_str()))
                .filter(|s| *s != "cases")
                .map(|s| format!("_{s}"))
                .unwrap_or_default();
            let base = match (real.is_some(), ids.is_some()) {
                (true, false) => "injection_real",
                (true, true) => "injection_real_subset",
                (false, false) => "injection_simulated",
                (false, true) => "injection_simulated_subset",
            };
            finish(
                &format!("{base}{suffix}"),
                injection::run_corpus_path(real_dyn, &label, ids.as_deref(), &corpus_path),
            )
        }
        "extraction" => {
            let split = arg(&args, "--split").unwrap_or_else(|| "heldout".into());
            match (real_dyn, model_arg.as_deref()) {
                (Some(m), _) => finish(
                    &format!("extraction_real_{split}{}", if ids.is_some() { "_subset" } else { "" }),
                    extraction::run(Some(m), &label, &split, ids.as_deref(), profile),
                ),
                (None, Some("oracle")) => finish(
                    &format!("extraction_oracle_{split}"),
                    extraction::run(None, "oracle", &split, None, profile),
                ),
                _ => {
                    eprintln!("tsr-eval extraction needs --model oracle (SIMULATED) or --model DIR (real files)");
                    std::process::exit(2);
                }
            }
        }
        "search" => finish(
            if emb_dyn.is_some() { "search_e5" } else { "search" },
            search::run(emb_dyn),
        ),
        "draft" => {
            let split = arg(&args, "--split").unwrap_or_else(|| "all".into());
            finish(
                &format!("draft_{}_{split}", if real.is_some() { "real" } else { "simulated" }),
                draft::run(real_dyn, &label, &split, profile),
            )
        }
        "ask" => {
            let split = arg(&args, "--split").unwrap_or_else(|| "all".into());
            finish(
                &format!(
                    "ask_{}_{split}{}",
                    if real.is_some() { "real" } else { "simulated" },
                    if ids.is_some() { "_subset" } else { "" }
                ),
                ask::run(real_dyn, emb_dyn, &label, &split, ids.as_deref(), profile),
            )
        }
        "rollup" => finish(
            if real.is_some() {
                "rollup_real"
            } else {
                "rollup_extractive"
            },
            rollup::run(real_dyn, &label),
        ),
        "seed" => match arg(&args, "--dest").map(PathBuf::from) {
            Some(d) => match seed::run(&d, arg(&args, "--canary").as_deref()) {
                Ok(msg) => {
                    println!("{msg} -> {}", d.display());
                    true
                }
                Err(e) => {
                    eprintln!("tsr-eval seed: {e}");
                    std::process::exit(2);
                }
            },
            None => {
                eprintln!("tsr-eval seed needs --dest DIR");
                std::process::exit(2);
            }
        },
        "backlog" => finish("backlog_simulated", backlog::run(profile)),
        "scorecard" => {
            let (_, json, md) = profile::scorecard(&common::root());
            let p = common::write_report("scorecard", &json, &md);
            println!(
                "{}\n-> {}",
                md.lines().take(3).collect::<Vec<_>>().join("\n"),
                p.display()
            );
            true
        }
        "all" => {
            let a = finish("injection_simulated", injection::run_corpus(None, "simulated", None));
            let b = finish(
                "extraction_oracle_heldout",
                extraction::run(None, "oracle", "heldout", None, profile),
            );
            let c = finish("search", search::run(None));
            let d = finish("backlog_simulated", backlog::run(profile));
            let (_, sc_json, sc_md) = profile::scorecard(&common::root());
            let _ = common::write_report("scorecard", &sc_json, &sc_md);
            a && b && c && d
        }
        _ => {
            println!(
                "{}",
                include_str!("main.rs")
                    .lines()
                    .take(16)
                    .map(|l| l.trim_start_matches("//!"))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
            true
        }
    };
    std::process::exit(if ok { 0 } else { 1 });
}
