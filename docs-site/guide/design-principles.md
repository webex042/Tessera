# Design Principles

Tessera is built on eleven core engineering principles that guide all architectural decisions, trade-offs, and implementation choices.

---

### 1. Hardware-Vendor Independence at the Integration Layer
- **Problem Addressed:** Coupling host applications to vendor-specific SDKs (CUDA, ROCm, SYCL) creates unmanageable deployment fragmentation.
- **Design Choice:** The host application links zero GPU libraries. Hardware backends exist as separate, pinned builds of `llama.cpp` dynamically arbitrated at runtime.
- **Implementation:** `crates/tsr-infer::hardware`, `backend`, `select`.

### 2. Strict Process Separation between Orchestration and Execution
- **Problem Addressed:** In-process inference crashes, driver faults, and VRAM leaks terminate the host application.
- **Design Choice:** The model executes inside an external sidecar process managed by an operating-system Job Object (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`).
- **Implementation:** `crates/tsr-infer::sidecar`, `crates/tsr-win::job`.

### 3. Data is Passive; Models Propose, Code Decides
- **Problem Addressed:** Indirect prompt injection where untrusted text commands the model to execute unwanted actions or fabricate commitments.
- **Design Choice:** Screen content is wrapped in rigid XML-style delimiters (`<screen_content>`). Models have zero tool execution privileges. Outputs are schema-constrained JSON validated deterministically by Rust code.
- **Implementation:** `crates/tsr-pipeline::guard`, `schema`, `audit`.

### 4. Local-First and Air-Gapped by Construction
- **Problem Addressed:** Unintentional data exfiltration and dependency on remote cloud infrastructure.
- **Design Choice:** All inference, embeddings, and database storage remain strictly on `127.0.0.1`. Outbound network access is confined to a single crate (`tsr-net`) requiring explicit user approval.
- **Implementation:** `crates/tsr-net`, `tools/check_network_deps.py`.

### 5. Multi-Rung Adaptive Acquisition
- **Problem Addressed:** High CPU and GPU cost of continuous full-screen OCR and vision-language processing.
- **Design Choice:** An adaptive acquisition ladder: Rung 1 (UI Automation, 285ms deadline) $\rightarrow$ Rung 2 (WGC + WinRT OCR) $\rightarrow$ Rung 3 (VLM window crop). Fast, lightweight structured methods are exhausted before falling back to computer vision.
- **Implementation:** `crates/tsr-capture::engine`, `uia`, `ocr`, `vlm`.

### 6. Zero Plaintext Exposure on Disk
- **Problem Addressed:** Sensitive credentials and personal information stored in unencrypted application databases or temporary files.
- **Design Choice:** Full database encryption via SQLCipher (AES-256-CBC) with keys protected by Windows DPAPI. Pre-storage regex and checksum redaction scrubs credentials before persistence. Screenshots are never written to disk.
- **Implementation:** `crates/tsr-store`, `crates/tsr-guard::redact`.

### 7. Resource-Aware, Preemptive Scheduling
- **Problem Addressed:** Long-running background summarization jobs starving interactive user requests and draining laptop batteries.
- **Design Choice:** Three priority tiers (P0 interactive, P1 extraction, P2 rollups). P0 requests preempt P1/P2 jobs via stream-drop socket termination. Background jobs defer on battery power.
- **Implementation:** `crates/tsr-infer::sched`.

### 8. Verifiable Citation Grounding
- **Problem Addressed:** Hallucinations in generated answers and daily summaries.
- **Design Choice:** Every displayed claim must cite an exact evidence quote validated as a substring ($Dice \ge 0.9$) within the original captured screen item. Unverified sentences are visually flagged.
- **Implementation:** `crates/tsr-retrieval::ask`, `crates/tsr-pipeline::evidence`.

### 9. Content-Free Diagnostic Observability
- **Problem Addressed:** Logging frameworks inadvertently recording passwords, private emails, or sensitive screen data.
- **Design Choice:** A custom `tracing` subscriber layer enforces an absolute allowlist of event names, durations, error codes, and bounded numeric metrics. All text content is wrapped in `Sensitive<T>` and dropped from logs.
- **Implementation:** `crates/tsr-core::logging`, `sensitive`.

### 10. Explicit Model Discovery and Setup
- **Problem Addressed:** Implicit, unexpected multi-gigabyte downloads during application startup.
- **Design Choice:** The application starts paused. Model downloads require explicit user approval (`--approve`). Existing local files can be pointed to directly and verified against cryptographic manifests without re-downloading.
- **Implementation:** `crates/tsr-net::download`, `tsr-models` CLI.

### 11. Testable Interfaces and Reproducible Builds
- **Problem Addressed:** Codebases that cannot be tested without specific physical GPU hardware or active internet connections.
- **Design Choice:** Pure platform-neutral state machines for scheduling and guardrails; mock adapter engines for all GPU vendors; synthetic evaluation datasets for pipeline scoring without live models.
- **Implementation:** `evals/`, `tests/fixtures/`, `crates/tsr-infer/tests/selection.rs`.
