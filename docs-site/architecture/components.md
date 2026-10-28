# Component Breakdown

Tessera distributes its logic across ten specialized Rust crates and two application shells. This modularity ensures strict separation of concerns, testability, and security air-gapping.

---

## Workspace Crate Matrix

| Crate / Module | Primary Responsibility | Key Files | External Dependencies |
|---|---|---|---|
| **`tsr-core`** | Domain types, `Sensitive<T>`, allowlist logging | `sensitive.rs`, `logging.rs` | `tracing`, `zeroize`, `serde` |
| **`tsr-win`** | Win32/WinRT safe wrappers, DPAPI, Job Objects | `dpapi.rs`, `job.rs`, `tcp.rs`, `events.rs` | `windows` (Win32/WinRT APIs) |
| **`tsr-capture`** | Capture ladder (UIA, WGC, OCR, VLM), sessions | `uia.rs`, `ocr.rs`, `deadline.rs`, `engine.rs` | `windows`, `image` |
| **`tsr-guard`** | Blocklists, regex/checksum redaction, claims | `redact.rs`, `blocklist.rs`, `claims.rs` | `regex` (Platform-neutral) |
| **`tsr-store`** | SQLCipher database, migrations, FTS5 | `migrate.rs`, `items.rs`, `facts.rs` | `rusqlite` (bundled-sqlcipher) |
| **`tsr-retrieval`** | Pinned ONNX Runtime, E5 embedder, RRF | `runtime.rs`, `embed.rs`, `hybrid.rs` | `fastembed`, `ort` (api-31) |
| **`tsr-infer`** | Hardware probing, sidecar supervisor, scheduler | `backend.rs`, `select.rs`, `sched.rs`, `sidecar.rs` | `serde_json` (Zero external HTTP) |
| **`tsr-net`** | Approved, resumable, checksum-checked downloads | `download.rs` | OS SChannel TLS (The only net crate) |
| **`tsr-pipeline`** | Ingest, structured fact extraction, dates/people | `extract.rs`, `guard.rs`, `dates.rs`, `people.rs` | Pure algorithmic code |
| **`tsr-draft`** | In-place ghost drafting, fact check, clipboard restore | `draft.rs`, `factcheck.rs` | `tsr-win::clipboard` |
| **`src-tauri`** | Native desktop shell, window positioning, tray | `main.rs`, `lib.rs`, `window.rs`, `services.rs` | `tauri`, `tauri-build` |
| **`ui/`** | React 19 + TypeScript + Tailwind CSS notch UI | `src/App.tsx`, `src/views/` | `react`, `tailwindcss`, `vite` |

---

## Detailed Subsystem Responsibilities

### Core Orchestration (`src-tauri` & `tsr-core`)
- Owns the native message loop and manages application lifecycle.
- Intercepts and validates typed IPC requests from the WebView2 frontend.
- Enforces content-free diagnostic logging by dropping unlisted event fields at compile and runtime.

### Data Capture & Safety (`tsr-capture` & `tsr-guard`)
- Monitors active desktop focus via `SetWinEventHook`.
- Enforces capture state gates (pausing on session lock, fullscreen presentation, or battery).
- Runs the text acquisition ladder, aborting hung UIA calls at 285ms.
- Executes pre-storage secret scrubbing using algorithmic checks (Luhn for cards, Verhoeff for Aadhaar).

### Storage & Retrieval (`tsr-store` & `tsr-retrieval`)
- Persists captured items, facts, and rollups into an encrypted SQLite database using SQLCipher (AES-256-CBC).
- Maintains synchronized full-text search indexes using SQLite FTS5 with BM25 scoring.
- Computes in-memory cosine similarities against 384-dimensional int8 vector embeddings produced by local ONNX Runtime 1.31.

### Inference & Scheduling (`tsr-infer` & `tsr-pipeline`)
- Detects GPU hardware via DXGI, checks binary integrity, and probes candidate backends.
- Encapsulates `llama-server` in a Windows Job Object and verifies port ownership via `GetExtendedTcpTable`.
- Dispatches requests across three priority tiers (P0, P1, P2) using socket-drop preemption.
- Verifies that model fact quotes are exact substrings within source screen items before persisting.
