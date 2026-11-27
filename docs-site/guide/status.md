# Implementation Status

This document provides a transparent, empirical breakdown of Tessera's implementation status, separating verified capabilities from work in progress and known defects.

> [!IMPORTANT] Hardware Baseline & Two Benchmark Profiles
> Measured local inference metrics below were recorded on a physical development machine equipped with an **AMD Ryzen 5 7530U (AMD Radeon Integrated Graphics via Vulkan)** running Windows 11 with quantized **`Qwen3-VL-4B-Instruct-Q4_K_M`**.
> The planned discrete target GPU (**RTX 4050 6 GB**) was unavailable. Results from this AMD iGPU baseline must **not** be advertised as RTX 4050 performance or generalized across all GPUs/iGPUs.
>
> Tessera tracks two separate profiles in `tsr-eval` (`--profile standard` vs. `--profile local-igpu-beta`):
> - **Standard Product Profile (RTX 4050 / Discrete GPU Target):** Original Stage 0–7 production thresholds.
> - **Local iGPU Beta Profile (`Qwen3-VL-4B` on AMD iGPU):** Evidence-validated quality & latency adjustments (`Ask ≥ 70%` blind, `Extraction F1 ≥ 0.60`, relaxed latency budgets), while **never lowering security requirements** (100% injection safety, 100% secret redaction, 0 fabricated citations, 0 unauthorized application network connections). Provisional thresholds of `≥ 65%` Ask and `≥ 0.50` F1 were **rejected** after variance/sample auditing, retaining honest `FAIL` verdicts.

---

## Stage-by-Stage Scorecard

| Stage | Name | Target Scope | Verified Status | Key Findings & Evidence |
|---|---|---|---|---|
| **Stage 0** | Feasibility & Model Benchmark | Model validation, capture rungs, hardware detection | <StatusBadge status="partial" /> | Tooling built and verified; real hardware tested on AMD Vulkan & CPU; open WebView2 finding ([ADR-0002](https://github.com/webex042/Tessera/blob/main/docs/adr/0002-webview2-outbound-connection.md)). |
| **Stage 1** | Foundation & Capture | Encrypted store, capture ladder, redaction, OS hooks | <StatusBadge status="implemented" /> | SQLCipher AES-256 + AES-256-GCM VectorDb, DPAPI key unwrapping, 285ms UIA deadline worker, 355/355 secret redaction (`0.41%` FP). |
| **Stage 2** | Understanding Pipeline | Extraction, injection defense, sidecar supervisor | <StatusBadge status="partial" /> | 0% hallucination, 0 schema-invalid rows; real model extraction F1 = 0.533 (`FAIL` on both profiles); historical real injection `123/152` (`24/39` held-out), deterministic guard `152/152` sim + `13/13` replay. |
| **Stage 3** | Floating Notch UI | 60 fps window, capture exclusion, panel state machine | <StatusBadge status="partial" /> | 60 fps verified (`16.8 ms` p95 frame), input-to-visual p95 `64 ms`; WGC/GDI pixel capture exclusion `100%` pass (`PARTIAL` overall pending Teams/Zoom/OBS tests). |
| **Stage 4** | Rollups & Open Loops | Hour/day/month map-reduce, morning brief | <StatusBadge status="partial" /> | Highlights resolve (0 invalid IDs); real 4B rollups `11/11` active hours (`0%` fallback); unlock trigger not exercised live. |
| **Stage 5** | Contextual Draft Assistant | UIA target, private clipboard restore, fact check | <StatusBadge status="partial" /> | Plain text insertion, password refusal, clipboard restore, and 0 unflagged claims verified; `17/20` valid drafts (`5.9 s` / `16.4 s` p50 on iGPU). |
| **Stage 6** | Ask with Sources | Keyword+fact retrieval, citation verification | <StatusBadge status="fail" /> | 0 fabricated citations (`PASS`); `66.7%` accuracy (`40/60`), `83.7%` verified (`41/49`), `25.2 s` p50 first token on AMD iGPU (`FAIL` on both profiles). |
| **Stage 7** | Security Hardening | Threat model, canary scans, network isolation | <StatusBadge status="partial" /> | 355/355 redaction, `0` app outbound connections (`PASS`), `<thread>` delimiter escaping fixed; `1` WebView2 HTTPS connection ([ADR-0002](https://github.com/webex042/Tessera/blob/main/docs/adr/0002-webview2-outbound-connection.md) `FAIL`), 24h elevated audit pending. |
| **Stage 8** | Audio & Translation | Whisper.cpp STT, local translation | <StatusBadge status="blocked" /> | Blocked pending MVP exit criteria and model download approvals. |
| **Stage 9** | Closed Beta | Content-free diagnostics bundle, telemetry audit | <StatusBadge status="partial" /> | Diagnostics bundle built and inspected; beta testing pending. |
| **Stage 10**| Release Engineering | Unsigned beta NSIS installer & portable ZIP (SHA-256) | <StatusBadge status="partial" /> | CycloneDX SBOM, NSIS installer, portable ZIP, SHA-256 manifest, updater gated ([ADR-0003](https://github.com/webex042/Tessera/blob/main/docs/adr/0003-updater-blocked-by-zip-advisories.md)). |

---

## Detailed Two-Profile Empirical Scorecard (`Standard` vs. `Local iGPU Beta`)

All measured figures below are preserved unmodified from physical testing on **AMD Ryzen 5 7530U (AMD Radeon iGPU, Vulkan)** running **`Qwen3-VL-4B-Instruct-Q4_K_M`**:

| Evaluated Metric | Measured Result | Standard Target (RTX 4050) | Local iGPU Beta Target | Local Status |
|---|---|---|---|---|
| **Ask Answer Correctness (60 q)** | **66.7%** (`40/60`) | $\ge 80\%$ (blind) | $\ge 70\%$ (blind; $\ge 65\%$ rejected) | <StatusBadge status="fail" /> |
| **Ask Citation Verification** | **83.7%** (`41/49`); **0** fabricated | $\ge 90\%$ verified; 0 fabricated | $\ge 85\%$ verified; 0 fabricated | <StatusBadge status="fail" /> |
| **Ask Latency (p50 first / total)** | **25,155 ms** / **25,540 ms** | $\le 2,500\text{ ms} / \le 10,000\text{ ms}$ | $\le 15,000\text{ ms} / \le 30,000\text{ ms}$ | <StatusBadge status="fail" /> |
| **Extraction F1 Score (58 held-out)** | **0.533** (49.5% P / 57.8% R) | $\ge 0.70$ | $\ge 0.60$ ($\ge 0.50$ rejected) | <StatusBadge status="fail" /> |
| **Extraction Hallucination & Validity** | **0 / 97 (0.0%)**; **0** invalid | $\le 3.0\%$ hallucinated; 0 invalid | $\le 3.0\%$ hallucinated; 0 invalid | <StatusBadge status="pass" /> |
| **Prompt Injection (Real 4B Corpus)** | **123 / 152** (`24/39` held-out) | 152/152 safe (0 failures) | 152/152 safe (non-negotiable) | <StatusBadge status="fail" /> |
| **Prompt Injection (Code Guard Replay)** | **152/152** sim; **13/13** real replay | 0 leaks | 0 leaks (hardened fail-closed) | <StatusBadge status="pass" /> |
| **Secret Redaction & FP Rate** | **355 / 355 (100.0%)**, **0.41%** FP | 100% redacted, $\le 2.0\%$ FP | 100% redacted, $\le 2.0\%$ FP | <StatusBadge status="pass" /> |
| **Notch UI Latency & Frame Rate** | **64 ms p95**; **16.8 ms** frame p95 | $\le 100\text{ ms}$; 60 fps | $\le 100\text{ ms}$; 60 fps | <StatusBadge status="pass" /> |
| **Screen Capture Exclusion** | **100%** WGC/GDI pixels; apps untested | Absent in all (Teams/Zoom/OBS) | Absent in all (Teams/Zoom/OBS) | <StatusBadge status="partial" /> |
| **Search Behaviour Checks** | **6/6** checks pass (sim hash R@10 1.0) | Behaviour pass + E5 eval | Behaviour pass (E5 unmeasured) | <StatusBadge status="partial" /> |
| **Application Outbound Network** | **0 connections** (`tessara.exe`, `llama-server.exe`) | 0 unauthorized | 0 unauthorized (24h pending) | <StatusBadge status="pass" /> |
| **WebView2 Runtime Network (ADR-0002)** | **1 HTTPS** (`2603:1046:c04::/48:443`) | 0 connections | 0 connections / documented block | <StatusBadge status="fail" /> |

---

## Why Provisional Quality Thresholds Were Rejected

To prevent threshold manipulation, the candidate `Local iGPU Beta` thresholds of `≥ 65%` Ask correctness and `≥ 0.50` Extraction F1 were audited and **not** adopted:

1. **Ask Accuracy (`66.7%` vs. candidate `≥ 65%`):** On `N = 60` questions, the 95% Wilson confidence interval for `40/60` is **`[54.1%, 77.3%]`** (a single question flip shifts accuracy by $1.7\%$). Furthermore, `ask/v2` was authored after observing `ask/v1` failures on the same 60 items without an unseen held-out Ask split, verified citation share was `83.7%` (`< 90%`), and p50 first-sentence latency was `25.2 s`. Retaining `FAIL` against an evidence-validated `≥ 70%` blind floor accurately reflects beta quality limits.
2. **Extraction F1 (`0.533` vs. candidate `≥ 0.50`):** On `58` held-out items (`83` gold facts), `0.533` sits within one standard error of `0.50` ($\text{SE} \approx 0.045$), precision was **below 50%** (`49.5%`, driven by `23` false-positive `note` facts out of `29` predicted notes — `20.7%` note precision), and repeated-run stability was not measured. A meaningful minimum floor (`≥ 0.60` F1) requires curbing noisy `note` extractions rather than lowering the bar below 50% precision.

---

## Untested Checks & Open Architectural Deficiencies

1. **Unrun Hardware & Long-Duration Tests:** Discrete RTX 4050 benchmarks, 24-hour elevated Sysmon/WFP + `pktmon` TLS SNI network capture, 24-hour pagefile canary scan, live WGC exclusion against **Microsoft Teams, Zoom, OBS, Snipping Tool, and Game Bar**, live E5 embedder search evaluation (`tsr-eval search` uses `HashEmbedder`), live real-model re-run of `tsr-eval injection --model` after recent guard hardening, and a 3rd unseen injection test set have not yet been executed.
2. **Small Local Model Extraction & Latency Limits:** `Qwen3-VL-4B-Instruct-Q4_K_M` on integrated graphics produces `25.2 s` Ask p50 first-sentence latency and excessive `note` false positives (`49.5%` precision). Optional cloud BYOK (`Groq`, `openai/gpt-oss-20b`, with mandatory local `mask_pii`) is available for systems lacking discrete GPU VRAM.
3. **WebView2 Platform Telemetry ([ADR-0002](https://github.com/webex042/Tessera/blob/main/docs/adr/0002-webview2-outbound-connection.md)):** Tessera's own Rust binaries and `llama-server` open **zero** unauthorized connections, but `msedgewebview2.exe` initiates one outbound HTTPS connection to Microsoft (`2603:1046:c04::/48:443`) per run despite SmartScreen and Chromium background-networking switches being disabled.
