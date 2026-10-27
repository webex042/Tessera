//! `tsr-eval`: Stage 2 evaluation runners over the real pipeline code (plan §18.3).
//!
//!     tsr-eval injection [--model DIR]          prompt-injection corpus (default: SIMULATED compromised model)
//!     tsr-eval extraction --model oracle|DIR [--split heldout|dev|all]
//!     tsr-eval search                           retrieval: keyword, vector (SIMULATED embedder), hybrid
//!     tsr-eval rollup [--model DIR]             week replay through rollups (no model: code-only)
//!     tsr-eval draft [--model DIR] [--split S]  20 draft scenarios (no model: SIMULATED drafter)
//!     tsr-eval ask [--model DIR] [--split S]    60 Ask questions (no model: SIMULATED oracle)
//!     tsr-eval backlog                          2-hour synthetic workload through the real scheduler
//!     tsr-eval all                              everything that needs no model (CI)
//!     tsr-eval seed --dest DIR                  synthetic store for manual UI checks (TSR_DATA_DIR=DIR)
//!
//! `--model DIR` uses real model files in DIR (verified against tools/models.lock) on the best
//! backend this machine has. Results go to evals/results/<name>.{json,md}. Exit code 1 if a
//! threshold fails, 2 if the run could not happen (e.g. no model).

mod ask;
mod backlog;
mod common;
mod draft;
mod extraction;
mod injection;
mod rollup;
mod search;
mod seed;

use std::path::PathBuf;

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
    let model_arg = arg(&args, "--model");
    // `--ids a,b,c`: run only these cases/items (diagnosis); reports are named `*_subset`.
    let ids: Option<Vec<String>> = arg(&args, "--ids").map(|s| s.split(',').map(str::to_owned).collect());
    let real = match model_arg.as_deref() {
        None | Some("oracle") | Some("simulated") => None,
        Some(dir) => match common::real_model(&PathBuf::from(dir)) {
            Ok(m) => Some(m),
            Err(e) => {
                eprintln!("tsr-eval: cannot start a real model: {e}");
                std::process::exit(2);
            }
        },
    };
    let label = real
        .as_ref()
        .map(|m| m.label.clone())
        .unwrap_or_else(|| "simulated".into());
    let real_dyn = real.as_ref().map(|m| m as &dyn tsr_infer::client::ChatModel);
    let ok = match cmd.as_str() {
        "injection" => finish(
            match (real.is_some(), ids.is_some()) {
                (true, false) => "injection_real",
                (true, true) => "injection_real_subset",
                (false, false) => "injection_simulated",
                (false, true) => "injection_simulated_subset",
            },
            injection::run_corpus(real_dyn, &label, ids.as_deref()),
        ),
        "extraction" => {
            let split = arg(&args, "--split").unwrap_or_else(|| "heldout".into());
            match (real_dyn, model_arg.as_deref()) {
                (Some(m), _) => finish(
                    &format!("extraction_real_{split}{}", if ids.is_some() { "_subset" } else { "" }),
                    extraction::run(Some(m), &label, &split, ids.as_deref()),
                ),
                (None, Some("oracle")) => finish(
                    &format!("extraction_oracle_{split}"),
                    extraction::run(None, "oracle", &split, None),
                ),
                _ => {
                    eprintln!("tsr-eval extraction needs --model oracle (SIMULATED) or --model DIR (real files)");
                    std::process::exit(2);
                }
            }
        }
        "search" => finish("search", search::run()),
        "draft" => {
            let split = arg(&args, "--split").unwrap_or_else(|| "all".into());
            finish(
                &format!("draft_{}_{split}", if real.is_some() { "real" } else { "simulated" }),
                draft::run(real_dyn, &label, &split),
            )
        }
        "ask" => {
            let split = arg(&args, "--split").unwrap_or_else(|| "all".into());
            finish(
                &format!("ask_{}_{split}", if real.is_some() { "real" } else { "simulated" }),
                ask::run(real_dyn, &label, &split),
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
            Some(d) => match seed::run(&d) {
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
        "backlog" => finish("backlog_simulated", backlog::run()),
        "all" => {
            let a = finish("injection_simulated", injection::run_corpus(None, "simulated", None));
            let b = finish(
                "extraction_oracle_heldout",
                extraction::run(None, "oracle", "heldout", None),
            );
            let c = finish("search", search::run());
            let d = finish("backlog_simulated", backlog::run());
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
