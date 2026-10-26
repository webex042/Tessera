# Tessara

A local-first, screen-aware memory app for Windows.

Tessara reads the text on your screen as you work, keeps it as private, searchable memory with a source for every item, and helps you answer questions, draft replies and keep track of what you promised. Everything runs on your own PC. Nothing is sent anywhere.

> **Status:** design phase. Nothing is implemented yet. See [`implementation_plan.md`](implementation_plan.md) for the technical design and [`stages.md`](stages.md) for the build stages.

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

These claims are tested in Stage 7 (24-hour network monitor, on-disk canary scan) before any release.

## Target hardware

- Windows 11
- NVIDIA GPU with 6 GB VRAM (RTX 4050 laptop class); planning range for model memory is 4.2–5.2 GB
- 16 GB RAM

Other GPUs (AMD/Intel via Vulkan) and CPU-only machines are planned as slower fallback modes.

## Tech stack

Tauri 2 (Rust) + React UI in WebView2 · `llama-server` sidecar running Qwen3-VL-4B-Instruct (Q4_K_M) · SQLCipher with FTS5 · `fastembed` (ONNX Runtime, CPU) for embeddings · Windows UI Automation, Windows Graphics Capture and Windows OCR for reading the screen.

## Repository layout

See [`implementation_plan.md` §3](implementation_plan.md#3-repository-layout).

## License

TBD.
