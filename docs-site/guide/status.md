---
title: Implementation Status
description: Transparent stage-by-stage engineering status and hardware-aware evaluation scorecard for Tessera.
---

# Implementation Status

This document summarizes Tessera's verified implementation progress across Stages 0–10 alongside the latest `2026-11-28` real-model scorecard. For full failure root causes, security boundaries, and untested items, see [Measured Status and Limitations](/guide/limitations).

> [!IMPORTANT] Hardware Baseline & Two Benchmark Profiles
> Measured local inference metrics below were recorded on a single development machine (**AMD Ryzen 5 7530U**, **AMD Radeon Integrated Graphics via Vulkan**, Windows 11, **`Qwen3-VL-4B-Instruct-Q4_K_M`**). The discrete **RTX 4050 (6 GB)** target was unavailable; integrated GPU numbers must **not** be presented as RTX 4050 performance or universal hardware guarantees.
>
> `tsr-eval` separates two profiles (`evals/runner/src/profile.rs`):
> - **Standard Product Profile (`--profile standard`):** Original Stage 0–7 RTX 4050 class quality and latency targets.
> - **Local iGPU Beta Profile (`--profile local-igpu-beta`):** Hardware-calibrated latency and beta quality thresholds for AMD Radeon iGPU (`Ask >= 70%` blind, `Extraction F1 >= 0.60`, `First sentence <= 15s`), with **zero weakening of security invariants**.

---

## Stage-by-Stage Implementation Matrix

| Stage | Name | Target Scope | Status | Verified Findings & Gaps |
|---|---|---|---|---|
| **Stage 0** | Feasibility & Model Benchmark | Hardware detection, GGUF check, capture & WebView2 audit | <StatusBadge status="partial" /> | `tsr-hw` and backend probing verified on AMD Vulkan & CPU; open WebView2 Microsoft HTTPS sockets ([ADR-0002](https://github.com/webex042/Tessera/blob/main/docs/adr/0002-webview2-outbound-connection.md)). |
| **Stage 1** | Foundation & Capture | SQLCipher, VectorDb, DPAPI, capture ladder, redaction | <StatusBadge status="implemented" text="pass" /> | SQLCipher AES-256 + AES-256-GCM `VectorDb` v2, DPAPI key wrapping, 285ms UIA deadline, `355/355` redaction (`0.41%` FP on `242` clean docs), retention pruning (`0–3650` d) & full wipe. |
| **Stage 2** | Understanding Pipeline | Fact extraction, injection guards, sidecar supervisor | <StatusBadge status="partial" /> | `0/133` (`0.0%`) hallucinations, `0` schema-invalid rows, `152/152` + `20/20` (`unseen_v3`) guarded injection **PASS**; blind held-out Extraction F1 = **0.500** (`FAIL`, vs `dev` `0.702`). |
| **Stage 3** | Floating Notch UI | 60 fps panel state machine, exclusion, focus return | <StatusBadge status="partial" /> | `64 ms` input-to-visual p95, `16.8 ms` frame p95, `20/20` focus return; `2026-10-27` pixel exclusion `PASS` (`2026-11-28` re-run `0x80070005` on locked desktop); Zoom/OBS `NOT TESTED`. |
| **Stage 4** | Rollups & Open Loops | Hour/day/month rollups, morning brief, open loops | <StatusBadge status="partial" /> | Real 4B rollups `11/11` active hours (`0.0%` fallback, `0` invalid IDs), no auto-close tests pass; OS unlock trigger not tested live. |
| **Stage 5** | Contextual Draft Assistant | UIA target check, clipboard restore, fact check | <StatusBadge status="partial" /> | `20/20` valid drafts, `0` unflagged unsupported claims, `5,587 ms` first / `14,175 ms` complete (`PASS` `local-igpu-beta`, `FAIL` `standard`); Gmail/Outlook/Slack/Teams live insertion & Win+V untested. |
| **Stage 6** | Ask with Sources | Hybrid `E5`+FTS5+facts retrieval, streaming citations | <StatusBadge status="pass" text="pass (beta)" /> | 3 independent blind `heldout` runs (`48` q): **77.1%** accuracy, **92.3%** verified, **0** fabricated citations, **14,240 ms** p50 first / **15,315 ms** full (`PASS` `local-igpu-beta`; `FAIL` vs `standard`). |
| **Stage 7** | Security Hardening | Threat model, redaction, injection, network isolation | <StatusBadge status="partial" /> | `355/355` redaction, `152/152` + `20/20` guarded injection, `0` app outbound connections, `0` `cargo-deny` advisories (default & `groq`); `msedgewebview2.exe` opened `3` TLS `443` sockets (`ADR-0002` `FAIL`), 24h elevated audit pending. |
| **Stage 8** | Audio & Translation | Local WASAPI, `whisper.cpp` STT, translation | <StatusBadge status="deferred" /> | Deferred until Stage 2 extraction F1 (`>= 0.60` / `0.70`) and Stage 7 security gates close. |
| **Stage 9** | Closed Beta | Content-free diagnostics bundle, dogfooding | <StatusBadge status="in-progress" /> | Diagnostics export implemented and tested (`PrivacyView` / `DiagnosticsView`); 2-week dogfood & 7-day stability not run. |
| **Stage 10** | Release Engineering | Installer, portable ZIP, checksums, SBOM, updater gate | <StatusBadge status="partial" /> | Unsigned `v0.0.1-beta` NSIS + portable ZIP + `SHA256SUMS.txt` published on GitHub Releases; CycloneDX SBOM built; Authenticode signing pending; updater disabled per `ADR-0003`. |

---

## Latest Empirical Scorecard (`2026-11-28`, Standard vs. Local iGPU Beta)

| Evaluated Metric | Latest Measured Result | Standard Target (`RTX 4050`) | Local iGPU Beta Target | Local Status |
|---|---|---|---|---|
| **Ask Correctness** (3×48 blind `heldout`) | **77.1% ± 0.0%** (`37/48`) | $\ge 80\%$ (blind) | $\ge 70\%$ (blind) | <StatusBadge status="pass" /> |
| **Ask Citation Verification** (3×48 blind) | **92.3% ± 0.0%** (`36/39`); **0** fabricated | $\ge 90\%$ verified; `0` fab | $\ge 85\%$ verified; `0` fab | <StatusBadge status="pass" /> |
| **Ask Latency** (p50 first / full, 3 runs) | **14,240 ± 516 ms** / **15,315 ± 490 ms** | $\le 2,500 / \le 10,000\text{ ms}$ | $\le 15,000 / \le 30,000\text{ ms}$ | <StatusBadge status="pass" /> |
| **Extraction F1** (58 blind `heldout`) | **0.500** (`40.6%` P / `65.1%` R; `dev` = `0.702`) | $\ge 0.70$ | $\ge 0.60$ | <StatusBadge status="fail" /> |
| **Extraction Hallucination & Validity** | **0 / 133 (0.0%)** hallucinated; **0** invalid | $\le 3.0\%$ hallucinated; `0` invalid | $\le 3.0\%$ hallucinated; `0` invalid | <StatusBadge status="pass" /> |
| **Prompt Injection (Real 4B Corpus, 152)** | **152 / 152 safe** app-level (`4` raw echoes blocked) | `152 / 152` safe (`0` failures) | `152 / 152` safe | <StatusBadge status="pass" /> |
| **Prompt Injection (`unseen_v3`, 20 cases)** | **20 / 20 safe** app-level (`0` raw echoes) | `0` unsafe stored outcomes | `0` unsafe stored outcomes | <StatusBadge status="pass" /> |
| **Draft Safety & Speed** (`20` scenarios) | **20/20** valid, **0** unflagged, **5,587 / 14,175 ms** | `20/20`, `0`, $\le 3,000 / \le 8,000\text{ ms}$ | `20/20`, `0`, $\le 6,000 / \le 20,000\text{ ms}$ | <StatusBadge status="pass" /> |
| **Search (`multilingual-e5-small` ONNX)** | **R@5 0.981**, **R@10 1.000**, **nDCG@10 0.921**, `6/6` checks | E5 eval + behaviour pass | E5 eval + behaviour pass | <StatusBadge status="pass" /> |
| **Secret Redaction & FP Rate** | **355 / 355 (100.0%)**, **1 / 242 (0.41%)** FP | `100%` redacted, $\le 2.0\%$ FP | `100%` redacted, $\le 2.0\%$ FP | <StatusBadge status="pass" /> |
| **Notch UI Latency & Frame Time** | **64 ms** input-to-visual p95; **16.8 ms** frame p95 | $\le 100\text{ ms}$; $60\text{ fps}$ | $\le 100\text{ ms}$; $60\text{ fps}$ | <StatusBadge status="pass" /> |
| **Screen Capture Exclusion** | `2026-10-27` WGC/GDI `1.0`; `2026-11-28` `0x80070005` locked | Absent in all capture tools | Absent in all capture tools | <StatusBadge status="partial" /> |
| **App Outbound Network** | **0 connections** (`tessara.exe`, `llama-server.exe`) | `0` unauthorized | `0` unauthorized (`24h` pending) | <StatusBadge status="partial" /> |
| **WebView2 Network (`ADR-0002`)** | NetLog `0` URLs; **3** TLS `443` sockets (`msedgewebview2.exe`) | `0` connections / blocked | `0` connections / documented SNI | <StatusBadge status="fail" /> |
| **Groq BYOK Live Evaluation** | Probe + unit tests only (`groq` optional build) | Verified live behavior | Documented opt-in BYOK | <StatusBadge status="not-tested" /> |

---

## Remaining Blockers Before Standard Production Gate

1. **Blind Extraction Precision (`F1 = 0.500` vs. `>= 0.60`):** `Qwen3-VL-4B-Instruct-Q4_K_M` emitted `42` false-positive `action` items and `21` false-positive `note` items across the 58 blind items (`40.6%` precision), causing a `FAIL` on both profiles despite `dev = 0.702`.
2. **Untested Discrete Hardware (`RTX 4050`) & Cloud BYOK (`Groq`):** No RTX 4050 or live Groq BYOK evaluation run has been completed; interactive speed below `2.5–3.0 s` remains unverified on discrete GPUs.
3. **WebView2 Telemetry & Unlocked Capture Audits:** Resolving `ADR-0002` requires an elevated `pktmon` TLS SNI capture and 24h Sysmon/WFP audit, and desktop capture exclusion requires unlocked re-testing with Teams, Zoom, OBS, Snipping Tool, and Xbox Game Bar.
