# Key Concepts

This document outlines the core architectural and systems concepts employed throughout Tessera.

---

## 1. The Isolated Inference Sidecar

Instead of compiling C++ inference runtimes into the primary application binary, Tessera manages an external `llama-server` process known as a **sidecar**:
- **Job Object Containment:** On Windows, the process is spawned with `CREATE_SUSPENDED` and bound to an operating-system Job Object configured with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`. When the parent application terminates, the OS automatically terminates the sidecar.
- **Environment Cleansing:** The child process starts with an empty environment, retaining only essential Windows variables (`SystemRoot`, `windir`, `TEMP`, `TMP`). Variables such as `LLAMA_ARG_AGENT` or `HF_TOKEN` are stripped.
- **Port-Owner Verification:** Communication takes place over `127.0.0.1`. Before trusting `http://127.0.0.1:<port>/health`, the host queries `GetExtendedTcpTable` to verify that the socket owner PID matches the spawned sidecar PID.

---

## 2. The Text Acquisition Ladder

Tessera captures desktop screen content using an adaptive 3-rung ladder, prioritizing structured access over expensive image processing:

```
Rung 1: UI Automation (COM MTA)
  └─► Fast, lightweight structured tree walk (< 285ms deadline)
  └─► Skips IsPassword controls structurally

Rung 2: Windows Graphics Capture (WGC) + WinRT OCR
  └─► Fallback when UIA times out or yields empty text
  └─► Blanks out password bounding boxes before OCR

Rung 3: Vision-Language Model (VLM Crop)
  └─► Invoked only when OCR quality score < 0.55 or on visual apps
  └─► In-memory downscaled PNG crop passed to multimodal model
```

---

## 3. Dynamic Backend Arbitration

Tessera defines an explicit distinction between:
- **Application Integration:** The stable Rust interface used by the application.
- **Hardware Detection:** Operating system queries (DXGI) enumerating physical adapters and VRAM without loading vendor runtimes.
- **Backend Probing:** Executing verified `llama-server --list-devices` binaries in a clean environment to test driver compatibility.
- **Model Compatibility:** Inspecting GGUF metadata headers without loading weight tensors.
- **Device Execution:** Actually compiling tensors and executing forward passes on hardware.

Candidates are ranked automatically:
$$\text{Full VRAM Fit} > \text{Discrete GPU} > \text{Vendor Preferred Backend} > \text{Available Free VRAM} > \text{CPU Fallback}$$

---

## 4. Priority Scheduler with Stream-Drop Preemption

Inference workloads are divided into three discrete priority queues:
- **P0 (Interactive):** User-facing tasks (Ask with Sources, Draft Assistant). Maximum wait target: $\le 300\text{ ms}$.
- **P1 (Near-Real-Time):** Background window extraction and VLM transcription. Target: $\le 5\text{ min}$.
- **P2 (Batch Maintenance):** Daily briefs and multi-day rollups. Target: $\le 24\text{ hr}$ (AC power preferred).

### Stream-Drop Preemption
When an interactive P0 request arrives while a P1 extraction is executing, the host drops the loopback HTTP/1.1 TCP stream. `llama-server` detects the socket disconnect, halts tensor generation, and frees the context. The preempted P1 job is re-queued with an incremented attempt count.

---

## 5. Deterministic Validation vs. Model Proposals

Tessera adheres to a fundamental principle: **Models propose; deterministic code validates.**
- **JSON Schema Grammar:** All model generation is constrained by formal JSON schemas converted into execution grammars by the inference runtime.
- **Substring Evidence Matching:** Every extracted fact must cite an exact evidence quote. The pipeline verifies that the quote exists verbatim or with $Dice \ge 0.9$ within the original captured screen item. Fabricated quotes are rejected.
- **Imperative Classification:** Screen text is treated strictly as untrusted data (`<screen_content>`). Injected commands are classified as `needs_review` or rejected entirely.

---

## 6. Hybrid Search & Reciprocal Rank Fusion (RRF)

Tessera combines lexical keyword retrieval and semantic vector retrieval:
- **Lexical Channel:** SQLite FTS5 index computing BM25 relevance scores.
- **Vector Channel:** Pinned ONNX Runtime 1.31 executing `multilingual-e5-small` to produce 384-dimensional int8 embeddings, evaluated via in-memory brute-force cosine similarity.
- **Fusion:** Scores are combined using Reciprocal Rank Fusion ($k=60$) modulated by an age-decay recency multiplier:
$$\text{Score} = \text{RRF}(k=60) \times \left(1 + 0.15 \times 0.5^{\frac{\text{age}}{14\text{ days}}}\right)$$

---

## 7. `Sensitive<T>` & Content-Free Auditing

To prevent accidental leakage of private screen text or secrets:
- All unredacted strings are wrapped in `Sensitive<T>`. Its `Debug` and `Display` implementations emit `<redacted>`.
- The logging framework (`tsr-core::logging`) enforces a strict allowlist (`ALLOWED_MESSAGES` and `ALLOWED_FIELDS`). Unlisted field names or values containing arbitrary strings are dropped at runtime.
