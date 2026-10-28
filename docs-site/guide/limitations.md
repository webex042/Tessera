# Known Limitations & Constraints

This document details the verified empirical boundaries, known technical limitations, and unachieved benchmark targets for the Tessera prototype.

Rather than claiming production readiness, Tessera follows a strict "measured, not assumed" engineering discipline. Every constraint documented below is backed by empirical test logs from the repository's test suites and evaluation ledgers (`docs/ledger.md`, `docs/final-report.md`).

---

## 1. MVP Gate Status & Unachieved Targets

As recorded in `docs/ledger.md` and `docs/final-report.md`, Stages 0 through 7 are implemented, but the overall MVP production readiness gate is currently **not met** due to three specific target failures:

| Metric / Evaluation Area | Target | Measured Result | Status | Root Cause |
| :--- | :--- | :--- | :--- | :--- |
| **Ask Correctness (60 queries, real 4B model)** | $\ge 80\%$ | **66.7%** | <StatusBadge status="fail" /> | Extraction errors in preceding pipeline stage; prompt context truncation |
| **Information Extraction F1 (held-out corpus)** | $\ge 0.70$ | **0.507** | <StatusBadge status="fail" /> | Compact 4B model struggling with complex multi-field schemas |
| **Network Isolation (10-minute audit)** | 0 connections | **1 outbound** | <StatusBadge status="fail" /> | Microsoft WebView2 runtime telemetry beacon (Tessera code: 0) |

### Analysis of Extraction and Ask Failures
* **Extraction F1 (0.507):** The 4B quantized parameter model (`Q4_K_M`) exhibits false negatives on complex structured schemas containing nested or ambiguous screen text (e.g., parsing tabular data or code diffs from editor windows).
* **Ask Groundedness vs Accuracy:** While the Ask pipeline scored **0 fabricated citations** (100% citation precision against verified screen text), it answered only 40 of 60 evaluation questions correctly (66.7%). When the retrieval or extraction stage missed context, the system safely declined to answer rather than hallucinating.

---

## 2. Platform & OS Constraints

* **Windows 11 Only:** The current implementation targets Windows 11 (64-bit x86_64). macOS (Darwin/Metal) and Linux (X11/Wayland) are not supported.
* **Windows Graphics Capture (WGC):** Requires Windows 10/11 build 19041 or higher. On older OS builds or unsupported virtualization environments, WGC fails, dropping the pipeline to the GDI fallback capture engine.
* **GDI Fallback Limitations:**
  * Cannot capture hardware-accelerated surfaces (DirectX, Vulkan, OpenGL, or DRM-protected content).
  * Susceptible to window occlusion: if another window overlaps the target application, GDI captures the occluding window's pixels.
  * Lacks per-window dirty rect tracking, requiring full frame reads.

---

## 3. Hardware & Acceleration Boundaries

* **No Dynamic Compilation:** Tessera strictly forbids dynamic runtime linking or host compilation against NVIDIA CUDA, AMD ROCm, or Intel oneAPI/SYCL.
* **Sidecar Architecture:** Hardware acceleration is provided exclusively via pre-built, pinned binary sidecars (`llama-b11900-bin-win-vulkan-x64.zip` and `llama-b11900-bin-win-cpu-x64.zip`).
* **Vulkan Driver Dependency:** The Vulkan backend requires Windows display drivers supporting Vulkan 1.2+. If driver initialization fails or VRAM allocation is rejected, Tessera automatically falls back to CPU inference.
* **CPU Inference Demands:** CPU fallback requires hardware support for **AVX2** and **FMA** vector instructions. On machines lacking AVX2, CPU token generation rates fall below interactive usability thresholds ($< 2$ tokens/second).

---

## 4. Model Architecture & Resource Constraints

* **Supported Format:** Tessera currently supports only **GGUF** format models compatible with `llama.cpp` build b11900. Proprietary checkpoints (SafeTensors, PyTorch `.pt`, ONNX) are not supported for primary text generation.
* **Model Sizing (4B Default):**
  * Weights footprint: $\approx 2.4 \text{ GB}$ disk space.
  * Runtime memory footprint: $\approx 3.8 \text{ GB}$ system RAM / VRAM under default 4096-token context.
  * Minimum recommended system RAM: **8 GB** (system struggles under concurrent browser loads).
  * Recommended system RAM: **16 GB** or higher.
* **Context Window Limitation:** Inference context is capped at 4096 tokens to prevent out-of-memory crashes on integrated GPUs (Intel Iris Xe, AMD Radeon 680M/780M). Long screen histories must be aggressively chunked and summarized.

---

## 5. Security & Isolation Boundaries

* **WebView2 Network Telemetry:**
  While Tessera's Rust binaries (`tsr-*`) and frontend code produce **zero** network requests (verified by `tools/netmon.ps1`), Microsoft's WebView2 browser control periodically dispatches an outbound telemetry heartbeat to Microsoft endpoints. Remediation via group policy flags and runtime switches is tracked in Stage 8.
* **DPAPI Encryption Scope:**
  Storage encryption relies on the Windows Data Protection API (DPAPI) tied to the logged-in Windows user account (`CryptProtectData`). Data is protected against offline drive theft and unprivileged processes, but is accessible to any process executing under the same user security context unless explicit access control lists (ACLs) are configured.
* **Screen Scraping Adversarial Injections:**
  Although `tsr-guard` tests successfully resisted prompt injection attacks in the Stage 7 audit, untrusted text displayed on screen remains an untrusted input vector. Content displayed on web pages or chat applications could attempt secondary prompt manipulation if extraction delimiters are compromised.

---

## 6. Next Steps & Mitigations

For detailed technical mitigation plans and architectural roadmaps addressing each limitation, consult:
* [Implementation Status](/guide/status) — Full test scorecards and stage completion matrix.
* [Roadmap](/guide/roadmap) — Stage 8 through Stage 10 milestones.
* [Threat Model](/privacy/threat-model) — Comprehensive security boundary analysis.
