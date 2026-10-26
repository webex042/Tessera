# Tessara

A local-first, screen-aware memory app for Windows.

Tessara reads the text on your screen as you work, keeps it as private, searchable memory with a source for every item, and helps you answer questions, draft replies and keep track of what you promised. Everything runs on your own PC. Nothing is sent anywhere.

> **Status:** Stage 0 (feasibility) in progress — tooling built and verified; model benchmarks
> pending the target GPU; one open network finding ([ADR-0002](docs/adr/0002-webview2-outbound-connection.md)).
> Not usable as an app yet. See [`docs/stage0/results.md`](docs/stage0/results.md),
> [`implementation_plan.md`](implementation_plan.md) and [`stages.md`](stages.md).

## What it does (MVP)

- **Remembers your work as text, with sources.** Every memory points back to where it came from (URL, file, or app and window title) and when.
- **Answers questions with checked citations.** Each sentence in an answer cites stored evidence that code has verified, or is marked "not verified".
- **Drafts replies in place.** Press a hotkey, get a draft grounded in the current thread and your memory, then insert, edit or discard it. Tessara never sends anything.
- **Shows the shape of your time.** Hour, day, week and month summaries, a morning brief, and a list of open loops (tasks, commitments, unanswered questions).

## What it does not do

- Send email or messages, click buttons in other apps, or act on its own.
- Connect to calendar, email or other APIs. It only knows what appeared on screen.
- Sync, accounts or telemetry.
- Run on macOS or Linux (v1 is Windows only).

## Privacy model

- **Local inference only.** The model runs in a local `llama-server` process on `127.0.0.1`. No cloud calls.
- **No screenshots or audio on disk.** Screen images exist only in memory for milliseconds, long enough to read text from them.
- **Encrypted store.** All memory lives in one SQLCipher database whose key is protected by Windows (DPAPI).
- **Redaction before storage.** Passwords, card numbers, Aadhaar/PAN, OTPs, API keys and similar secrets are removed before anything is stored or shown to the model.
- **Blocklists.** Password managers, banking and login pages, and private/incognito browser windows are skipped entirely. If Tessara can't tell whether a browser window is private, it treats it as private.
- **Visible pause.** One hotkey stops all capture. The app starts paused until you finish onboarding and press Start.
- **Auditable network boundary.** Only one crate (`tsr-net`) can reach the internet, and only when you ask it to (model download, update check).

These are design goals. They are tested in Stage 7 (24-hour network monitor, on-disk canary scan) before any release, and [`docs/privacy.md`](docs/privacy.md) tracks what is true of the code today — including the WebView2 connection that Stage 0 found and has not yet removed.

## Target hardware

- Windows 11
- NVIDIA GPU with 6 GB VRAM (RTX 4050 laptop class); planning range for model memory is 4.2–5.2 GB
- 16 GB RAM

Other GPUs (AMD/Intel via Vulkan) and CPU-only machines are planned as slower fallback modes.

## Tech stack

Tauri 2 (Rust) + React UI in WebView2 · `llama-server` sidecar running Qwen3-VL-4B-Instruct (Q4_K_M) · SQLCipher with FTS5 · `fastembed` (ONNX Runtime, CPU) for embeddings · Windows UI Automation, Windows Graphics Capture and Windows OCR for reading the screen.

## Building from source (Stage 0)

Prerequisites: Windows 11, Rust stable (MSVC), Visual Studio Build Tools with C++, Node.js 24 +
pnpm 12, Python 3.11+, and Strawberry Perl on `PATH` (the portable zip works; SQLCipher's vendored
OpenSSL needs it — the first build takes ~25 minutes).

```powershell
pnpm install
pnpm build:ui
./tools/fetch-sidecar.ps1 -Component onnxruntime -Variant win-x64   # pinned, hash-checked
cargo test --workspace -- --include-ignored
cargo build -p tessara --features custom-protocol
python evals/validate.py
python -m unittest discover -s bench/tests
python tools/check_network_deps.py --locked
cargo deny check
```

Stage 0 checks and how to reproduce each one: [`docs/stage0/results.md`](docs/stage0/results.md).

## Repository layout

See [`implementation_plan.md` §3](implementation_plan.md#3-repository-layout). What exists today:
`crates/tsr-store`, `crates/tsr-retrieval`, `crates/tsr-infer/prompts`, `src-tauri`, `ui`,
`bench/` (benchmark harness and capture probe), `evals/` (synthetic eval sets), `tools/`, `docs/`.

## License

TBD.
