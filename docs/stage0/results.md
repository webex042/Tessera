# Stage 0 results (2026-10-26)

**Stage 0 status: IN PROGRESS — not passed.** The machine available today is not the target
hardware (AMD Radeon iGPU, 512 MB dedicated VRAM; no NVIDIA GPU), and by decision of the project
owner no model files were downloaded or run. Every model-dependent gate is therefore
**UNVERIFIED**, with a ready-to-run procedure below. Nothing here is a pass by assumption.

## Exit criteria (stages.md, Stage 0)

| Criterion | Target | Result | Status |
|---|---|---|---|
| VRAM peak with vision at 8k ctx | ≤ 5.2 GB on RTX 4050 6 GB | not measured (no target GPU, no model) | UNVERIFIED |
| Extraction JSON validity | ≥ 99 % | not measured | UNVERIFIED |
| Task/commitment F1 | ≥ 0.65 (MVP 0.70) | not measured | UNVERIFIED |
| Extraction p50 latency (1 500-token item) | ≤ 8 s | not measured | UNVERIFIED |
| Ask p50 time to first token | ≤ 2.5 s | not measured | UNVERIFIED |
| Draft blind self-rating | ≥ 3/5 | not measured | UNVERIFIED |
| UIA or OCR usable for target apps | ≥ 16 of 20 | **7 of 7 tested usable**; 13 not testable here | UNVERIFIED (7/20) |
| WebView2 network spike (no unexpected outbound) | 0 connections | **1 connection per run to Microsoft 443** | **FAIL** (ADR-0002) |
| Dependency spike: no network crates outside tsr-net | 0 paths | 0 paths (default and updater builds) | PASS |

## What was measured for real

| Check | Command | Result |
|---|---|---|
| SQLCipher + FTS5 encrypted round trip, wrong key, no plaintext on disk | `cargo test -p tsr-store` | 4/4 pass; SQLCipher 4.19.0 / SQLite 3.53.4 / openssl provider |
| Pinned ONNX Runtime loads from exact path, System32 copy not mapped, tampered/relative refused | `cargo test -p tsr-retrieval -- --include-ignored` | 4/4 pass |
| Network crates unreachable outside tsr-net | `python tools/check_network_deps.py --locked [--features tessara/updater]` | PASS both |
| Checker rejects fastembed/ort default features | fixtures in `tests/fixtures/netdeps/` | both rejected (exit 1) |
| cargo-deny, default build | `cargo deny check` | advisories, bans, licenses, sources ok |
| cargo-deny, updater build | `cargo deny --features tessara/updater check` | **FAIL**: zip 4.6.1 RUSTSEC-2026-0367/0368/0369 |
| WebView2 settings read back from a live webview | `TSR_STAGE0_EXIT_AFTER_S=20 tessara.exe --stage0-report` | runtime 155.0.4283.62; SmartScreen off; wry defaults kept; planted override cleared |
| WebView2 network (polling + NetLog) | `tools/netmon.ps1`, `TSR_STAGE0_NETLOG` | WPAD found and removed; 1 non-NetLog 443 connection remains |
| UIA / WGC / OCR on 7 installed apps (synthetic fixtures) | `tools/run-capture-probe.ps1` | all usable; UIA timeouts not wall-clock (Word 2.8 s) |
| Pinned binaries re-verify | `tools/fetch-sidecar.ps1` | archive + extracted-file hashes match lock |
| Harness pipeline + scorer (SIMULATED server) | `python -m unittest discover -s bench/tests` | 11/11 pass; gold answers score F1 = 1.0; injected noise detected |
| Eval sets well-formed | `python evals/validate.py` | 80 extraction items / 126 facts, 60 Ask, 20 Draft; all evidence verbatim |
| Rust fmt / clippy `-D warnings` | `cargo fmt --check`, `cargo clippy --workspace --all-targets` | clean |
| UI typecheck + build | `pnpm build:ui` | clean, no inline scripts |

## How to finish Stage 0 (any GPU, or CPU)

With the model files placed locally (download is a user decision; record their SHA-256 in
`tools/sidecar.lock` under a new `models` section):

```powershell
python tools/fetch_sidecar.py --list                         # builds for each backend
python tools/fetch_sidecar.py llama_cpp <variant> --record   # the build for this machine's GPU (once, then commit)
cargo run --release -p tsr-infer --bin tsr-hw -- --model <path>/Qwen3-VL-4B-Instruct-Q4_K_M.gguf --mmproj <path>/mmproj-Qwen3-VL-4B-Instruct-F16.gguf
# take <variant>, --device and -ngl from candidate 1 of tsr-hw's plan, then:
python bench/run_bench.py --server third_party/llama_cpp/b11900/<variant>/llama-server.exe `
  --model <path>/Qwen3-VL-4B-Instruct-Q4_K_M.gguf --mmproj <path>/mmproj-Qwen3-VL-4B-Instruct-F16.gguf `
  --device <ID> --ngl all --label q4km-ctx8k-q8kv-<backend>-<gpu> --split heldout
```

For CUDA, also fetch `cudart-12.4-x64` (it unpacks into the CUDA build) and re-record the CUDA
manifest. `bench/results/<date>-<label>/summary.md` fills in the gate table automatically (GPU
memory per llama-server process from Windows counters, any vendor; `nvidia-smi` additionally on
NVIDIA), and `drafts_for_rating.md` is the blind-rating sheet. Repeat with `--kv-type f16`,
`--ctx 4096` and every backend that will ship (`docs/hardware-backends.md`). On the dev machine
(AMD iGPU) the candidate today is `vulkan-x64`, `--device Vulkan0 --ngl all`.

Still to build before Stage 0 can close (not started in this session):

- `bench/embed-eval`: recall@10 / MRR for multilingual-e5-small vs bge-small-en-v1.5 on the Ask
  set (needs local model files).
- OCR vs VLM character error rate on rendered fixtures (Windows OCR half can run without a model).
- The 2-hour and 24-hour network runs with an elevated capture (`network-test.md`).
