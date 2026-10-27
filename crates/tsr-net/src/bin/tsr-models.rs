//! `tsr-models`: the onboarding model-setup steps from the command line (the app's onboarding
//! screens call the same library functions).
//!
//!     tsr-models check [--third-party DIR]          hardware check (detected / installed / verified)
//!     tsr-models list                               models in tools/models.lock
//!     tsr-models locate DIR [--verify]              check a folder you already have
//!     tsr-models plan ID                            what a download would fetch, from where
//!     tsr-models fetch ID --dest DIR --approve      download (only with --approve)
//!
//! Output is JSON. `fetch` without `--approve` prints the plan and exits without any network
//! request.

use std::path::PathBuf;

use tsr_infer::backend::LlamaServerProbe;
use tsr_infer::cancel::CancelToken;
use tsr_infer::hardware::SystemAdapters;
use tsr_infer::models::{MANIFEST, entry, inspect_dir};
use tsr_infer::onboard::{cpu_info, estimated_needs, hardware_check, memory};
use tsr_infer::select::Override;
use tsr_net::download::{Approval, DownloadConfig, download_model, plan_download};

fn fail(msg: &str) -> ! {
    eprintln!("tsr-models: {msg}");
    std::process::exit(2)
}

fn print(v: &impl serde::Serialize) {
    println!("{}", serde_json::to_string_pretty(v).expect("json"));
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |f: &str| args.iter().any(|a| a == f);
    let value = |f: &str| args.iter().position(|a| a == f).and_then(|i| args.get(i + 1)).cloned();
    match args.first().map(String::as_str) {
        Some("check") => {
            let tp = PathBuf::from(value("--third-party").unwrap_or_else(|| "third_party".into()));
            let report = tsr_infer::detect(&tp, &SystemAdapters, &LlamaServerProbe::default());
            let hc = hardware_check(
                &report,
                cpu_info(),
                memory(),
                estimated_needs(&MANIFEST),
                true,
                &[],
                &Override::default(),
            );
            print(&hc);
        }
        Some("list") => print(&*MANIFEST),
        Some("locate") => {
            let dir = args.get(1).unwrap_or_else(|| fail("locate needs a folder"));
            let r = inspect_dir(
                &PathBuf::from(dir),
                &MANIFEST,
                flag("--verify"),
                &CancelToken::new(),
                &mut |f, p| eprint!("\rverifying {f}: {} / {} MiB   ", p.done >> 20, p.total >> 20),
            );
            eprintln!();
            print(&r);
        }
        Some("plan") => {
            let id = args.get(1).unwrap_or_else(|| fail("plan needs a model id"));
            print(&plan_download(entry(id).unwrap_or_else(|| fail("unknown model id"))));
        }
        Some("fetch") => {
            let id = args.get(1).unwrap_or_else(|| fail("fetch needs a model id"));
            let e = entry(id).unwrap_or_else(|| fail("unknown model id"));
            let dest = value("--dest").unwrap_or_else(|| fail("fetch needs --dest DIR"));
            print(&plan_download(e));
            if !flag("--approve") {
                eprintln!("Not downloading: add --approve to confirm the download above.");
                std::process::exit(1);
            }
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            let approval = Approval::by_user(&[id.as_str()], now);
            let r = download_model(
                e,
                &approval,
                &DownloadConfig::new(PathBuf::from(dest)),
                &CancelToken::new(),
                &mut |p| {
                    eprint!(
                        "\r{} {:?}: {} / {} MiB   ",
                        p.file,
                        p.phase,
                        p.done >> 20,
                        p.total >> 20
                    )
                },
            );
            eprintln!();
            match r {
                Ok(out) => print(&out),
                Err(e) => fail(&e.to_string()),
            }
        }
        _ => {
            println!(
                "{}",
                include_str!("tsr-models.rs")
                    .lines()
                    .take(10)
                    .map(|l| l.trim_start_matches("//!"))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }
    }
}
