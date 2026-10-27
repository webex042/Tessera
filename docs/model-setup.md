# Model setup (onboarding)

How Tessara finds hardware, gets model files, and starts the local model. GPU-vendor details
are in [hardware-backends.md](hardware-backends.md); decisions in
[ADR-0006](adr/0006-stage2-understanding-pipeline.md).

Status (2026-10-27): the steps below exist as library functions and as the `tsr-models` command
line tool. The onboarding *screens* in the app are not built yet (Stage 1 pending item).

## 1. Hardware check

```bash
cargo run -p tsr-net --bin tsr-models -- check
```

Reports, without loading any model or GPU runtime from the internet:

| level | meaning | evidence |
|---|---|---|
| detected | the OS lists the adapter | DXGI (Windows), sysfs (Linux) |
| installed | a pinned llama.cpp build for that backend is on disk and every file matches `tools/sidecar.lock` | file manifest |
| verified | that build's backend initialised and listed the device | `llama-server --list-devices` |
| inference-tested | a real model answered on that device | `onboard::smoke_test` (needs model files) |

Also: OS, architecture, CPU features (AVX2, AVX-512, …), total and free memory, and an estimate
of what the model needs (from the manifest sizes before download, from the GGUF headers after).
A machine without a usable GPU gets the verdict **CPU only (slow mode)** — never a rejection.
Extraction then runs only on AC power when idle.

Measured on the dev machine (AMD Radeon integrated GPU, 12 logical cores with AVX2/FMA/F16C,
15.3 GiB RAM): CPU build verified, Vulkan build
verified on `Vulkan0` (AMD Radeon Graphics); verdict GPU (Vulkan). No model has run yet, so no
backend is "inference-tested".

## 2. Use files you already have

```bash
cargo run -p tsr-net --bin tsr-models -- locate "D:\models" --verify
```

Looks only in the folder you give (and the manifest's own sub-paths such as `onnx/`), never
elsewhere. Never copies, moves, renames or deletes anything. For each file in
`tools/models.lock` it reports *missing*, *incomplete* (shorter than published — an interrupted
download), *wrong size*, *not a readable GGUF model file*, *digest mismatch*, or OK. With
`--verify` every present file is hashed (SHA-256, or git-blob SHA-1 for the small JSON configs)
against the manifest; without it only sizes and headers are checked. Files the manifest does not
list are named so you know they were seen and why they are not used.

## 3. Download (only with your approval)

```bash
cargo run -p tsr-net --bin tsr-models -- plan qwen3-vl-4b-instruct-q4_k_m
cargo run -p tsr-net --bin tsr-models -- fetch qwen3-vl-4b-instruct-q4_k_m --dest "D:\models" --approve
```

`plan` shows host, files, sizes and licence. Without `--approve`, `fetch` prints the plan and
makes **no** request. The app's onboarding will show the same plan and create the approval only
from your click.

Guarantees (`crates/tsr-net/src/download.rs`, 13 tests against a simulated server):

- HTTPS only, OS TLS and certificate store, no proxies; redirects followed by hand and only to
  Hugging Face hosts.
- Expected size and digest come from the compiled-in manifest, never from the server.
- Resumable with `Range`; a server that ignores ranges, sends a wrong `Content-Range` or too many
  bytes never gets its data spliced in — the download restarts from zero. A digest mismatch after
  a resume restarts once from zero, then fails.
- Data goes to `<name>.part` with a `<name>.part.json` description; the final name appears only
  by rename after the whole file verified. An existing different file with that name is never
  overwritten.
- Paths come from the manifest and are checked (no `..`, drive letters, `:` streams, reserved
  device names); the folder cannot be escaped through a junction.

Models in the manifest: Qwen3-VL-4B-Instruct Q4_K_M (2.33 GiB), its F16 vision projector
(0.78 GiB, needed for rung 3), multilingual-e5-small ONNX (0.46 GiB, the embedder).

## 4. Start and test

The sidecar is started by `tsr-infer::sidecar` with the backend chosen by
`tsr-infer::select` (best GPU candidate, CPU always last). Then `onboard::smoke_test` sends one
fixed tiny prompt; success marks that backend *inference-tested*.

## Supported backends

| backend | where | status on the dev machine |
|---|---|---|
| CPU (AVX2/AVX-512, ARM64) | everywhere | build verified; no model run |
| Vulkan | AMD, Intel, NVIDIA, Qualcomm on Windows/Linux | build verified on AMD iGPU; no model run |
| CUDA | NVIDIA | pinned in the lock, not installed here (no NVIDIA GPU) |
| ROCm / HIP | AMD (supported cards) | pinned, not installed here |
| SYCL, OpenVINO | Intel | pinned, not installed here |
| Metal | Apple | pinned (macOS builds), untested |
| OpenCL | Qualcomm Adreno | pinned, untested |

The embedder runs on the CPU (pinned ONNX Runtime 1.31 CPU build). Nothing in the codebase
requires NVIDIA hardware or CUDA.

## Troubleshooting

| symptom | cause | fix |
|---|---|---|
| `tsr-models check` lists the GPU as *detected* only | no pinned build for its backend installed | `python tools/fetch_sidecar.py llama_cpp vulkan-x64` (or the vendor build) |
| *installed* but not *verified* | driver too old or backend fails to initialise | update the GPU driver; Tessara falls back to the next backend, then CPU |
| `locate` says *incomplete* | an interrupted download | run `fetch` again: it resumes from the `.part` file |
| `locate` says *digest mismatch* | the file is a different revision or damaged | download again with `fetch` (it will not overwrite your file: move it away first) |
| "model unavailable" after restarts | the sidecar crashed or hung 5 times in an hour | check free memory; Tessara keeps capturing and keyword search keeps working; *retry* resets the budget |
| everything slow | CPU only | expected; extraction waits for AC power and idle time |
