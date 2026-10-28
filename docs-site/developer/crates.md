# Crate Ecosystem Index

Tessera divides its functionality across 10 internal Rust crates, an application shell, and supporting binaries.

---

## 1. Internal Crates

### `tsr-core`
- **Location:** `crates/tsr-core/`
- **Purpose:** Foundational types, zeroizing memory wrappers, and content-free logging.
- **Key Exports:** `Sensitive<T>`, `logging::init_allowlist_logging()`.

### `tsr-win`
- **Location:** `crates/tsr-win/`
- **Purpose:** Safe Win32/WinRT abstractions for Windows 11.
- **Key Exports:** `dpapi::protect_key()`, `dpapi::unprotect_key()`, `job::JobObject`, `tcp::get_listening_pid()`, `display::set_capture_exclusion()`, `events::WinEventListener`.

### `tsr-capture`
- **Location:** `crates/tsr-capture/`
- **Purpose:** Screen text acquisition ladder, change detection, and sessions.
- **Key Exports:** `engine::CaptureEngine`, `uia::read_uia()`, `ocr::read_wgc_ocr()`, `vlm::crop_window()`, `simhash::SimHash64`, `session::SessionActor`.

### `tsr-guard`
- **Location:** `crates/tsr-guard/`
- **Purpose:** Platform-neutral window blocklists, secret redaction, and claim verification.
- **Key Exports:** `blocklist::is_window_blocked()`, `redact::redact_secrets()`, `claims::validate_claims()`.

### `tsr-store`
- **Location:** `crates/tsr-store/`
- **Purpose:** SQLCipher 4 database management, schema migrations, and FTS5 search.
- **Key Exports:** `open_encrypted()`, `migrate::run_migrations()`, `items::ItemRepository`, `facts::FactRepository`.

### `tsr-retrieval`
- **Location:** `crates/tsr-retrieval/`
- **Purpose:** Pinned ONNX Runtime loading, E5 embeddings, and Reciprocal Rank Fusion.
- **Key Exports:** `runtime::load_pinned()`, `embed::LocalEmbedder`, `hybrid::hybrid_search()`.

### `tsr-infer`
- **Location:** `crates/tsr-infer/`
- **Purpose:** Hardware detection, GGUF parsing, sidecar supervision, and priority scheduling.
- **Key Exports:** `hardware::detect()`, `backend::installed_builds()`, `select::rank_backends()`, `supervisor::SidecarSupervisor`, `sched::SchedulerCore`, `http::LoopbackClient`.

### `tsr-net`
- **Location:** `crates/tsr-net/`
- **Purpose:** The sole network-capable crate; approved model downloads.
- **Key Exports:** `download::download_model()`, `Approval`, `DownloadConfig`.

### `tsr-pipeline`
- **Location:** `crates/tsr-pipeline/`
- **Purpose:** Extraction jobs, prompt injection defense, date and entity resolution.
- **Key Exports:** `extract::ExtractionJob`, `guard::classify_instructions()`, `dates::resolve_relative_dates()`, `people::resolve_entities()`.

### `tsr-draft`
- **Location:** `crates/tsr-draft/`
- **Purpose:** Contextual ghost drafting, claim fact-checking, and private clipboard restore.
- **Key Exports:** `draft::generate_draft()`, `factcheck::verify_draft_claims()`.

---

## 2. Shells & Supporting Tools

- **`src-tauri`:** The desktop application entry point, window management, and Tauri IPC commands.
- **`bench/uia-probe`:** Standalone CLI tool measuring UIA and OCR latencies across native Windows applications.
- **`evals/runner` (`tsr-eval`):** Evaluation binary testing pipeline accuracy, injection defense, and retrieval metrics.
