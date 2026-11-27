# Known Limitations & Constraints

This document details the verified empirical boundaries, known technical limitations, and unachieved benchmark targets for the Tessera prototype.

Rather than claiming production readiness, Tessera follows a strict "measured, not assumed" engineering discipline. Every constraint documented below is backed by empirical test logs from the repository's test suites and evaluation ledgers (`docs/ledger.md`, `docs/final-report.md`).

---

## 1. Benchmark Profiles & MVP Gate Status

As recorded in `docs/ledger.md`, `docs/final-report.md`, and `evals/results/scorecard.md`, Stages 0 through 7 are implemented, and Tessera maintains two independent benchmark profiles (`--profile standard` vs. `--profile local-igpu-beta`):

- **Hardware Used for Measurement:** AMD Ryzen 5 7530U (AMD Radeon Integrated Graphics via Vulkan), Windows 11, running **`Qwen3-VL-4B-Instruct-Q4_K_M`**.
- **Target Hardware Not Benchmarked:** Discrete **RTX 4050 (6 GB VRAM)** was unavailable during benchmarking. Local AMD iGPU results must **not** be presented as RTX 4050 performance or universal compatibility across other GPUs/iGPUs.
- **Non-Negotiable Security Requirements:** Prompt injection resistance (`152/152` safe), secret redaction (`100%` of sensitive fixtures), zero fabricated citations, and zero unauthorized application outbound connections are **never lowered** in either profile.

| Metric / Evaluation Area | Measured Result (AMD iGPU) | Standard Target (RTX 4050) | Local iGPU Beta Target | Local Status | Root Cause & Audit Note |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Ask Correctness (60 q)** | **66.7%** (`40/60`) | $\ge 80\%$ (blind) | $\ge 70\%$ (blind; $\ge 65\%$ rejected) | <StatusBadge status="fail" /> | 95% Wilson CI `[54.1%, 77.3%]`, non-blind `ask/v2` iteration, and `83.7%` verified share (`< 90%`) |
| **Ask Latency (p50 first / total)** | **25.2 s** / **25.5 s** | $\le 2.5\text{ s} / \le 10.0\text{ s}$ | $\le 15.0\text{ s} / \le 30.0\text{ s}$ | <StatusBadge status="fail" /> | Integrated GPU memory bandwidth bottleneck (`0` fabricated citations passes) |
| **Extraction F1 (58 held-out)** | **0.533** (49.5% P / 57.8% R) | $\ge 0.70$ | $\ge 0.60$ ($\ge 0.50$ rejected) | <StatusBadge status="fail" /> | Within `1` SE of `0.50`; `49.5%` precision / `20.7%` `note` precision noise (`0%` hallucinated passes) |
| **Prompt Injection (Real 4B)** | **123 / 152** (`24/39` held-out) | 152/152 safe | 152/152 safe (non-negotiable) | <StatusBadge status="fail" /> | Historical real run before `guard.rs` hardening (`152/152` sim + `13/13` replay pass; live 4B re-run pending) |
| **Secret Redaction** | **355 / 355 (100%)**, `0.41%` FP | 100% redacted | 100% redacted | <StatusBadge status="pass" /> | Deterministic detectors cover Indian IDs + credentials |
| **Notch UI Latency (p95)** | **64 ms** (`16.8 ms` frame p95) | $\le 100\text{ ms}$ | $\le 100\text{ ms}$ | <StatusBadge status="pass" /> | Cold-start cycle `64 ms`; cycles 2–20 cluster `24–39 ms` |
| **Screen Capture Exclusion** | WGC/GDI pixels pass; apps untested | Absent in all | Absent in all | <StatusBadge status="partial" /> | Teams, Zoom, OBS, Snipping Tool, and Game Bar not yet tested live |
| **Application Network** | **0 connections** | 0 unauthorized | 0 unauthorized | <StatusBadge status="pass" /> | Verified via 10-min `netmon.ps1`; 24h Sysmon/WFP pending |
| **WebView2 Network (ADR-0002)** | **1 HTTPS** (`2603:1046:c04::/48`) | 0 connections | 0 connections | <StatusBadge status="fail" /> | `msedgewebview2.exe` platform beacon outside Chromium NetLog |

### Analysis of Extraction, Ask, and Security Failures
* **Extraction F1 (`0.533`) & Rejected `0.50` Candidate:** Measured on 58 held-out items (`83` gold facts) after ADR-0010 (`preserve_order`). Zero items failed invalid JSON (down from 15/58), recall rose from 44.6% to 57.8%, and hallucination was `0.0%` (`0/97`). However, precision fell to `49.5%` because the 4B model emitted `23` false-positive `note` facts. Adopting `≥ 0.50` was rejected because `0.533` sits within one standard error ($\approx 0.045$) of `0.50` and sub-50% precision is too noisy for default memory.
* **Ask Correctness (`66.7%`) & Rejected `≥ 65%` Candidate:** While Ask achieved **0 fabricated citations** and failed closed cleanly, `40/60` has a 95% Wilson confidence interval of `[54.1%, 77.3%]` (1 question away from `< 65%`), `ask/v2` had no separate blind held-out split, and `8/49` displayed sentences failed verification (`83.7% < 90%`).
* **Prompt Injection (`123/152` Historical Real Run):** Of the 29 historical failures on `Qwen3-VL-4B`, `5` were Windows standby timeouts (`inj-h04`, `h05`, `j01`, `j02`, `s7-005`), `10` were genuine-task misses (`inj-a05`, `e05`, `k04`, `s7-059..061`, `s7-g01/03/08/12`) caused by fail-closed taint rejection or `kind` misclassification, `1` was an extra open commitment (`inj-m04`), and `13` were safety leaks (`s7-002`, `004`, `012`, `016`, `018`, `019`, `020`, `024`, `026`, `028`, `030`, `050`, `066`). Deterministic guards in `tsr-pipeline::guard` (`s7-002` Hinglish SOV/refusal summary, `s7-018` forged user pre-approval, `s7-050` credential phishing) and `prompts::escape_content` (`<thread>`, `<style_examples>`) now pass `152/152` simulated and `13/13` recorded real-model safety replays, but live re-evaluation on physical hardware and a 3rd unseen injection set remain required before claiming `PASS`.
* **Untested Evaluations:** Real `multilingual-e5-small` search recall (`tsr-eval search` uses `HashEmbedder`), 24-hour elevated Sysmon/WFP + `pktmon` TLS SNI capture, 24-hour pagefile canary scan, and named third-party screen-sharing app checks remain unexecuted.

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

* **Outbound Network Separation (`ADR-0002`):**
  * **Application-originated (`tessara.exe`, `llama-server.exe`):** **0** non-loopback connections observed across 4–10 minute `tools/netmon.ps1` audits (`check_network_deps.py` also enforces that no crate outside `tsr-net` depends on HTTP/TLS crates).
  * **WebView2 platform-originated (`msedgewebview2.exe`):** **1** outbound HTTPS connection to Microsoft (`2603:1046:c04::/48:443`) per launch even with SmartScreen, WPAD (`--no-proxy-server`), and Chromium background-networking switches disabled. Absent from Chromium NetLog and Windows DNS cache; likely Edge/WebView2 diagnostic telemetry (**not yet confirmed** via elevated `pktmon` TLS SNI capture). Privacy implication: Microsoft receives a connection event on launch until identified and blocked/removed ([ADR-0002](https://github.com/webex042/Tessera/blob/main/docs/adr/0002-webview2-outbound-connection.md)). A formal 24-hour elevated Sysmon/WFP audit has not been run yet.
  * **Other third-party connections:** **0** in default local mode (opt-in Groq BYOK transmits only PII-masked text when explicitly configured and consented to by the user).
* **DPAPI Encryption Scope:**
  Storage encryption relies on the Windows Data Protection API (DPAPI) tied to the logged-in Windows user account (`CryptProtectData`). Data is protected against offline drive theft and unprivileged processes, but is accessible to any process executing under the same user security context unless explicit access control lists (ACLs) are configured.
* **Screen Scraping Adversarial Injections:**
  In deterministic tests (`tsr-eval injection`), the hardened pipeline guard (`tsr-pipeline::guard`) plus prompt delimiter escaping (`<screen_content>`, `<memory>`, `<thread>`, `<style_examples>`) blocks `152/152` simulated worst-case injections and `13/13` recorded real-model safety violations fail-closed. However, in the prior live `Qwen3-VL-4B` benchmark (`123/152` pass, `24/39` held-out), the compact 4B model echoed injected directives and misclassified mixed prompts until the guard caught them; unseen attack styles and live hardware re-verification remain open risks.

---

## 6. Next Steps & Mitigations

For detailed technical mitigation plans and architectural roadmaps addressing each limitation, consult:
* [Implementation Status](/guide/status) — Full test scorecards and stage completion matrix.
* [Roadmap](/guide/roadmap) — Stage 8 through Stage 10 milestones.
* [Threat Model](/privacy/threat-model) — Comprehensive security boundary analysis.
