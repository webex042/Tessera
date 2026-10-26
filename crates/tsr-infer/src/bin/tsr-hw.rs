//! `tsr-hw`: show what Tessara detects and which inference backend it would use.
//!
//!     cargo run -p tsr-infer --bin tsr-hw -- [--model M.gguf [--mmproj P.gguf]] [--ctx 8192] [--kv q8_0|f16]
//!         [--backend auto|cuda|rocm|vulkan|sycl|openvino|metal|opencl|cpu] [--device ID|N|name]
//!         [--gpu-layers N] [--no-fallback] [--third-party DIR] [--json]
//!
//! Read-only: lists adapters, verifies pinned builds, runs each build's `--list-devices` probe and
//! prints the ranked plan. It never downloads anything and never loads a model's weights.

use std::path::PathBuf;

use tsr_infer::backend::LlamaServerProbe;
use tsr_infer::gguf::read_gguf;
use tsr_infer::hardware::SystemAdapters;
use tsr_infer::select::{ModelNeeds, Override, plan};

fn main() {
    let mut args = std::env::args().skip(1);
    let (mut model, mut mmproj, mut ctx, mut kv) = (None::<PathBuf>, None::<PathBuf>, 8192u64, 1.0625f64);
    let (mut backend, mut device, mut layers, mut fallback, mut json) = (None, None, None, true, false);
    let mut third_party = PathBuf::from("third_party");
    while let Some(a) = args.next() {
        let mut val = || args.next().unwrap_or_else(|| fail(&format!("{a} needs a value")));
        match a.as_str() {
            "--model" => model = Some(val().into()),
            "--mmproj" => mmproj = Some(val().into()),
            "--ctx" => ctx = val().parse().unwrap_or_else(|_| fail("--ctx must be a number")),
            "--kv" => {
                kv = match val().as_str() {
                    "f16" => 2.0,
                    "q8_0" => 1.0625,
                    _ => fail("--kv must be f16 or q8_0"),
                }
            }
            "--backend" => backend = Some(val()),
            "--device" => device = Some(val()),
            "--gpu-layers" => layers = Some(val()),
            "--no-fallback" => fallback = false,
            "--third-party" => third_party = val().into(),
            "--json" => json = true,
            "-h" | "--help" => {
                println!(
                    "{}",
                    include_str!("tsr-hw.rs").lines().take(9).collect::<Vec<_>>().join("\n")
                );
                return;
            }
            other => fail(&format!("unknown argument {other}")),
        }
    }
    if !json {
        tracing_subscriber::fmt()
            .with_target(false)
            .with_writer(std::io::stderr)
            .init();
    }
    let ovr = Override::parse(backend.as_deref(), device.as_deref(), layers.as_deref(), fallback)
        .unwrap_or_else(|e| fail(&e.to_string()));

    let needs = match &model {
        Some(m) => {
            let g = read_gguf(m).unwrap_or_else(|e| fail(&format!("model: {e}")));
            let p = mmproj
                .as_ref()
                .map(|p| read_gguf(p).unwrap_or_else(|e| fail(&format!("mmproj: {e}"))));
            ModelNeeds::from_gguf(&g, p.as_ref(), ctx, kv)
        }
        // Plan §7.2 estimate for Qwen3-VL-4B Q4_K_M + F16 mmproj at 8k context, q8_0 KV.
        None => ModelNeeds {
            weights_mib: 2560,
            mmproj_mib: 820,
            kv_mib: 615,
            n_layers: 36,
            tensor_types: [
                tsr_infer::gguf::GgmlType::Q4K,
                tsr_infer::gguf::GgmlType::Q6K,
                tsr_infer::gguf::GgmlType::F16,
            ]
            .into(),
        },
    };

    let report = tsr_infer::detect(&third_party, &SystemAdapters, &LlamaServerProbe::default());
    let result = plan(&report.adapters_result(), &report.builds, &report.probes, &needs, &ovr);
    if json {
        let out = serde_json::json!({
            "report": report,
            "model_needs": needs,
            "model_is_estimate": model.is_none(),
            "plan": result.as_ref().map_err(|e| e.to_string()),
        });
        println!("{}", serde_json::to_string_pretty(&out).unwrap());
        return;
    }

    println!("\nOS {} {}", report.os, report.arch);
    match &report.adapters {
        Ok(a) if a.is_empty() => println!("Adapters: none reported"),
        Ok(a) => {
            for x in a {
                println!(
                    "Adapter {}: {} [{}] dedicated {} MiB, shared {} MiB{}",
                    x.index,
                    x.name,
                    x.vendor.as_str(),
                    x.dedicated_mib,
                    x.shared_mib,
                    if x.software { ", software" } else { "" }
                );
            }
        }
        Err(e) => println!("Adapters: {e}"),
    }
    for b in &report.builds {
        println!("Build {} ({}) verified", b.variant, b.backend.as_str());
    }
    for n in &report.build_notes {
        println!("Build {} NOT used: {}", n.variant, n.reason);
    }
    for (v, r) in &report.probes {
        match r {
            Ok(d) if d.is_empty() => println!("Probe {v}: backend initialised, no devices"),
            Ok(d) => {
                for x in d {
                    println!(
                        "Probe {v}: {} {} total {:?} MiB free {:?} MiB",
                        x.id, x.name, x.total_mib, x.free_mib
                    );
                }
            }
            Err(e) => println!("Probe {v}: FAILED {e}"),
        }
    }
    println!(
        "Model needs{}: weights {} MiB + vision {} MiB + KV {} MiB, {} layers, types {:?}",
        if model.is_none() { " (estimate, no --model)" } else { "" },
        needs.weights_mib,
        needs.mmproj_mib,
        needs.kv_mib,
        needs.n_layers,
        needs.tensor_types
    );
    match result {
        Ok(p) => {
            for (i, c) in p.candidates.iter().enumerate() {
                println!(
                    "{}. {}  [{}]  args: {}",
                    i + 1,
                    c.label(),
                    c.reason,
                    c.llama_args().join(" ")
                );
            }
            for n in &p.notes {
                println!("note: {n}");
            }
        }
        Err(e) => {
            println!("No plan: {e}");
            std::process::exit(2);
        }
    }
}

fn fail(msg: &str) -> ! {
    eprintln!("tsr-hw: {msg}");
    std::process::exit(2)
}
