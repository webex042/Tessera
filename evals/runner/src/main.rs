//! `tsr-eval`: Stage 2 evaluation runners over the real pipeline code (plan §18.3).
//!
//!     tsr-eval injection [--model DIR]          prompt-injection corpus (default: SIMULATED compromised model)
//!     tsr-eval extraction --model oracle|DIR [--split heldout|dev|all]
//!     tsr-eval search                           retrieval: keyword, vector (SIMULATED embedder), hybrid
//!     tsr-eval backlog                          2-hour synthetic workload through the real scheduler
//!     tsr-eval all                              everything that needs no model (CI)
//!
//! `--model DIR` uses real model files in DIR (verified against tools/models.lock) on the best
//! backend this machine has. Results go to evals/results/<name>.{json,md}. Exit code 1 if a
//! threshold fails, 2 if the run could not happen (e.g. no model).

mod backlog;
mod common;
mod extraction;
mod injection;
mod search;

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
            if real.is_some() {
                "injection_real"
            } else {
                "injection_simulated"
            },
            injection::run_corpus(real_dyn, &label),
        ),
        "extraction" => {
            let split = arg(&args, "--split").unwrap_or_else(|| "heldout".into());
            match (real_dyn, model_arg.as_deref()) {
                (Some(m), _) => finish(
                    &format!("extraction_real_{split}"),
                    extraction::run(Some(m), &label, &split),
                ),
                (None, Some("oracle")) => finish(
                    &format!("extraction_oracle_{split}"),
                    extraction::run(None, "oracle", &split),
                ),
                _ => {
                    eprintln!("tsr-eval extraction needs --model oracle (SIMULATED) or --model DIR (real files)");
                    std::process::exit(2);
                }
            }
        }
        "search" => finish("search", search::run()),
        "backlog" => finish("backlog_simulated", backlog::run()),
        "all" => {
            let a = finish("injection_simulated", injection::run_corpus(None, "simulated"));
            let b = finish("extraction_oracle_heldout", extraction::run(None, "oracle", "heldout"));
            let c = finish("search", search::run());
            let d = finish("backlog_simulated", backlog::run());
            a && b && c && d
        }
        _ => {
            println!(
                "{}",
                include_str!("main.rs")
                    .lines()
                    .take(12)
                    .map(|l| l.trim_start_matches("//!"))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
            true
        }
    };
    std::process::exit(if ok { 0 } else { 1 });
}
