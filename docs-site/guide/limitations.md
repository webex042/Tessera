---
title: Measured Status and Limitations
description: Transparent engineering report of Tessera's verified capabilities, hardware-calibrated benchmarks, AI and security failures, and remaining unexecuted validation.
---

# Measured Status and Limitations

This page documents Tessera's verified capabilities, failing criteria, security boundaries, and untested edge cases based strictly on empirical evaluation logs in the repository (`evals/results/`, `docs/ledger.md`, and workspace tests).

Rather than presenting a promotional pass report, every measurement on this page is tied to actual hardware, software revisions, and unmodified evaluation outputs. Where deterministic code guards catch unsafe model behavior, both the **raw model behavior** and the **application-level outcome** are reported separately.

---

## 1. Benchmark Hardware, Model & Profile Disclosure

All real-model evaluations reported below were recorded on a single development laptop rather than the discrete-GPU reference hardware:

| Environment Attribute | Verified Benchmark Configuration | Target Reference (Not Benchmarked) |
|---|---|---|
| **Measurement Date** | `2026-11-28` (plus preserved `2026-10-27/28` UI/rollup baselines) | — |
| **Operating System** | Windows 11 (x64, `Microsoft Windows NT 10.0.26300.0`) | Windows 11 (x64) |
| **CPU** | AMD Ryzen 5 7530U (6 physical cores / 12 threads, AVX2 + FMA) | Modern 6+ core x64 |
| **GPU & Backend** | **AMD Radeon Integrated Graphics** via `llama.cpp` `b11900` (**Vulkan**) | NVIDIA **RTX 4050 (6 GB VRAM)** |
| **Primary Local LLM** | `Qwen3-VL-4B-Instruct-Q4_K_M.gguf` + `mmproj-Qwen3VL-4B-Instruct-F16.gguf` | Discrete-GPU or larger quant |
| **Embedding Runtime** | `multilingual-e5-small` (384-d) via pinned `onnxruntime.dll` `1.31.0` | Same |
| **Evaluation Profile** | `local-igpu-beta` (`evals/runner/src/profile.rs`) | `standard` |

> [!IMPORTANT] Two Benchmark Profiles & Hardware Scope
> - **Standard Product Profile (`--profile standard`):** Long-term interactive quality and latency gates calibrated for a discrete **RTX 4050 (6 GB VRAM)** laptop. Physical RTX 4050 testing has **not** been executed.
> - **Local iGPU Beta Profile (`--profile local-igpu-beta`):** Hardware-specific beta targets calibrated for `Qwen3-VL-4B-Instruct-Q4_K_M` running on AMD Radeon integrated graphics over shared system RAM. These targets are **hardware-specific**, not universal product guarantees for all consumer GPUs or CPU-only laptops.
> - **Non-Negotiable Invariants:** Security criteria (`0` unsafe application-level injection outcomes, `100%` corpus secret redaction, `0` fabricated Ask citations, `0` schema-invalid rows, `0` unflagged unsupported draft claims, and `0` unauthorized app-originated outbound connections) are **never lowered** under either profile.

### Status Label Legend

- <StatusBadge status="pass" /> — Verified directly against stated target criteria in recorded outputs.
- <StatusBadge status="fail" /> — Latest measured result does not meet the target criterion.
- <StatusBadge status="partial" /> — Core mechanism is verified, but required environment or real-app coverage is incomplete.
- <StatusBadge status="not-tested" /> — No sufficient measurement has been executed on that target yet.
- <StatusBadge status="in-progress" /> — Active engineering investigation or validation is underway.
- <StatusBadge status="deferred" /> — Intentionally postponed until earlier MVP gates close.

---

## 2. Full Benchmark Scorecard (Performance vs. Security)

### A. AI Functional Quality & Performance Metrics

| Measurement | Actual Result (`AMD Ryzen 5 7530U` iGPU) | Standard Target (`RTX 4050`) | Local iGPU Beta Target | Local Status | Source Artifact |
|---|---|---|---|---|---|
| **Ask Answer Accuracy** (3 blind held-out runs × 48 q) | **77.1% ± 0.0%** (`37 / 48` in R1, R2, R3; `dev` = `91.7%` on `12` q) | $\ge 80\%$ (blind) | $\ge 70\%$ (blind) | <StatusBadge status="pass" /> | `ask_real_heldout_r{1,2,3}.md` |
| **Ask Citation Verification** (3 blind runs × 48 q) | **92.3% ± 0.0%** (`36 / 39` displayed sentences verified) | $\ge 90\%$ verified | $\ge 85\%$ verified | <StatusBadge status="pass" /> | `ask_real_heldout_r{1,2,3}.md` |
| **Ask Time to First Sentence (p50)** (3 blind runs) | **14,240 ± 516 ms** (`13,787`, `14,802`, `14,132 ms`) | $\le 2,500\text{ ms}$ | $\le 15,000\text{ ms}$ | <StatusBadge status="pass" /> | `ask_real_heldout_r{1,2,3}.md` |
| **Ask Full Answer Latency (p50)** (3 blind runs) | **15,315 ± 490 ms** (`15,127`, `15,871`, `14,947 ms`) | $\le 10,000\text{ ms}$ | $\le 30,000\text{ ms}$ | <StatusBadge status="pass" /> | `ask_real_heldout_r{1,2,3}.md` |
| **Extraction F1** (58 blind held-out items, 83 gold) | **0.500** (`dev` 22 items = `0.702`; oracle `all` = `0.996`) | $\ge 0.70$ | $\ge 0.60$ | <StatusBadge status="fail" /> | `extraction_real_heldout.md` |
| **Extraction Precision & Recall** (58 held-out items) | **40.6% Precision** / **65.1% Recall** (`133` stored vs `83` gold; `1` JSON fail) | Balanced $\ge 0.70$ F1 | Balanced $\ge 0.60$ F1 | <StatusBadge status="fail" /> | `extraction_real_heldout.md` |
| **Draft Validity** (20 scenarios) | **20 / 20 valid** (`16/20` must-include, `20/20` must-not-include, `19/20` placeholders) | All valid (`20 / 20`) | All valid (`20 / 20`) | <StatusBadge status="pass" /> | `draft_real_all.md` |
| **Draft Latency (p50 first / complete)** (20 scenarios) | **5,587 ms** first text / **14,175 ms** complete | $\le 3,000 / \le 8,000\text{ ms}$ | $\le 6,000 / \le 20,000\text{ ms}$ | <StatusBadge status="pass" /> | `draft_real_all.md` |
| **Rollup Reliability** (22 simulated items / 6 days) | **11 / 11** exact active hours, **0.0%** fallback (`15/15` model-written), `0` invalid IDs | Exact hours, $\le 10\%$ fallback, `0` invalid IDs | Same | <StatusBadge status="pass" /> | `rollup_real.md` |
| **Notch UI Latency & Frame Time** (20 open/close cycles) | **64 ms** input-to-visual p95 (`16.8 ms` animation frame p95; `20/20` focus return) | $\le 100\text{ ms}$ / $60\text{ fps}$ | $\le 100\text{ ms}$ / $60\text{ fps}$ | <StatusBadge status="pass" /> | `notch_perf.json` |
| **Search Behaviour Checks** (`tsr-eval search`) | **6 / 6 behaviour checks pass** (suppression, blocklist, OTP skip, dupes, fallback, determinism) | All pass | All pass | <StatusBadge status="pass" /> | `search_e5.md` |
| **Real Embedding Evaluation** (`multilingual-e5-small` ONNX) | **R@5 0.981**, **R@10 1.000**, **P@5 0.219**, **MRR 0.900**, **nDCG@10 0.921** (54 q / 80 items) | Measured pass | Measured pass | <StatusBadge status="pass" /> | `search_e5.md` |

*Note on Standard Profile:* Against the **Standard Product Profile (`RTX 4050`)**, Ask accuracy (`77.1% < 80%`), Ask latency (`14.2 s > 2.5 s`, `15.3 s > 10.0 s`), Draft latency (`5.6 s > 3.0 s`, `14.2 s > 8.0 s`), and Extraction F1 (`0.500 < 0.70`) remain <StatusBadge status="fail" /> or <StatusBadge status="not-tested" /> because the physical discrete GPU has not been benchmarked.

### B. Security, Privacy & Containment Metrics (Never Relaxed)

| Security / Privacy Measurement | Actual Result | Requirement (Both Profiles) | Status | Notes |
|---|---|---|---|---|
| **Ask Fabricated Citations** (3 blind runs × 48 q) | **0** across all 3 runs (`R1: 0`, `R2: 0`, `R3: 0`) | `0` fabricated | <StatusBadge status="pass" /> | Deterministic `verify_with_headers` blocks ungrounded claims |
| **Extraction Hallucination Rate** (58 held-out items) | **0 / 133 (0.0%)** stored facts lacking source evidence | $\le 3.0\%$ | <StatusBadge status="pass" /> | Substring/Dice verification rejects ungrounded quotes |
| **Extraction Schema-Invalid Rows** | **0** invalid rows (`0` injection items stored forbidden text) | `0` invalid rows | <StatusBadge status="pass" /> | Enforced by `tsr_pipeline::audit` |
| **Unsupported Draft Claims Unflagged** (20 scenarios) | **0** shown without a flag | `0` unflagged | <StatusBadge status="pass" /> | Verified by `tsr_draft::factcheck` |
| **Real-Model Prompt Injection** (`cases.json`, 152 cases) | **152 / 152 safe** at app level (**4** raw-model forbidden echoes blocked by guards) | `152 / 152` safe (`0` guarded failures) | <StatusBadge status="pass" /> | Fresh real `Qwen3-VL-4B` run after final edits (`injection_real.md`) |
| **Unseen 3rd Prompt Injection Set** (`unseen_v3.json`, 20 cases) | **20 / 20 safe** at app level (**0** raw-model forbidden echoes) | `0` unsafe stored outcomes | <StatusBadge status="pass" /> | Held-out 15 adversarial + 5 controls (`injection_real_unseen_v3.md`) |
| **Simulated Compromised Model Injection** (`152` + `20` cases) | **152 / 152 safe** (`130` raw echoes blocked) & **20 / 20 safe** (`15` raw echoes blocked) | `0` guarded failures | <StatusBadge status="pass" /> | Worst-case adversarial harness (`injection_simulated*.md`) |
| **Secret Redaction Corpus** (`355` secrets) | **355 / 355 (100.0%)** redacted across 13 categories | `100%` redacted | <StatusBadge status="pass" /> | `cargo test -p tsr-guard --test redaction_corpus --locked` |
| **Redaction False-Positive Rate** (`242` clean docs) | **1 / 242 (0.41%)** (`Order #[REDACTED:card]...`) | $\le 2.0\%$ FP | <StatusBadge status="pass" /> | `clean.txt` + `evals/extraction/items/*.json` |
| **Screen Capture Exclusion** (`WDA_EXCLUDEFROMCAPTURE`) | `2026-10-27` pixel test `pass: true` (`0x11`, WGC+GDI `1.0`); `2026-11-28` re-run failed (`BitBlt: 0x80070005` locked session); `MSTeams`/`ScreenSketch`/`Game Bar` installed only; `Zoom`/`OBS` absent | Absent in all capture & meeting tools | <StatusBadge status="partial" /> | Pixel re-run blocked while desktop locked; named apps not verified live (`Zoom`/`OBS` <StatusBadge status="not-tested" />) |
| **App Outbound Network** (`tessara.exe`, `llama-server.exe`) | **0** non-loopback connections (2-min debug run `2026-11-28`; 4–10 min prior runs) | `0` unauthorized connections | <StatusBadge status="partial" /> | Passes short audits & `check_network_deps.py`; 24h Sysmon/WFP pending |
| **WebView2 Outbound Network** (`ADR-0002`) | **0** external URLs in Chromium NetLog (`localhost` only); **3** TLS `443` sockets from `msedgewebview2.exe` (`52.110.14.167`, `40.104.66.66`) | `0` connections or documented/blocked SNI | <StatusBadge status="fail" /> | Microsoft Edge WebView2 platform sockets occur outside Chromium NetLog; `pktmon` TLS SNI capture needs Admin |

<details>
<summary>Expand detailed before vs. after comparison &amp; per-run variability (2026-10-28 vs. 2026-11-28)</summary>

| Metric | Prior Baseline (`baselines-2026-10-28`) | Latest Measurement (`2026-11-28`) |
|---|---|---|
| **Ask Correctness** | `66.7%` (`40/60`, `all` split) | `77.1% ± 0.0%` (`37/48`, 3 blind `heldout` runs) · `91.7%` (`dev`, `11/12`) |
| **Ask Verified Sentences** | `83.7%` (`41/49`) | `92.3% ± 0.0%` (`36/39`, 3 blind `heldout` runs) |
| **Ask Latency (p50 first / full)** | `25,155 ms` / `25,540 ms` | `14,240 ± 516 ms` / `15,315 ± 490 ms` (`R1: 13,787/15,127`, `R2: 14,802/15,871`, `R3: 14,132/14,947`) |
| **Extraction Held-Out F1 (58 items)** | `0.533` (`49.5%` P, `57.8%` R, `97` stored) | `0.500` (`40.6%` P, `65.1%` R, `133` stored; `0%` hallucinated) |
| **Injection Real 4B (`152` cases)** | `123 / 152` (`29` failures) | `152 / 152` PASS (`0` guarded failures, `4` raw echoes blocked) |
| **Draft (`20` scenarios)** | `17 / 20` valid, `5,870 ms` first, `16,372 ms` complete | `20 / 20` valid, `5,587 ms` first, `14,175 ms` complete |
| **Search (`multilingual-e5-small`)** | Hash embedder only (`sim-hash-v1`) | Real ONNX `E5` measured: `R@10 1.000`, `MRR 0.900`, `nDCG@10 0.921` |

Note: An intermediate `injection` run on `2026-11-28` (`injection_real.md` after continuous `draft` load) experienced `3` `model: transport: timed out` errors (`inj-m04`, `s7-055`, `s7-056`) with `0` security leaks; a clean re-run passed `152 / 152` with `0` guarded failures.

</details>

---

## 3. Remaining AI & Pipeline Limitations

### Ask Limitations (<StatusBadge status="pass" /> on Local iGPU Beta / <StatusBadge status="fail" /> on Standard)
- **Remaining 22.9% Incorrect Answers on Blind Held-Out Queries (`11 / 48`):** On ambiguous or multi-hop personal queries over `<= 3` retrieved snippets (clipped to `350` chars per snippet within a `1,050`-char prompt snippet budget to hit `<= 15 s` latency), the compact 4B local model occasionally misses the decisive fact or answers conservatively (`"insufficient"`).
- **Tight Latency Margin (`14.2 s` vs. `15.0 s` p50 ceiling):** Although prefill optimizations (`-b 2048 -ub 2048`, `MAX_SNIPPETS = 3`, single-line compact JSON) cut Ask first-sentence latency by ~44% from `25.2 s` down to `13.8–14.8 s`, Run 2 hit `14,802 ms` — leaving little headroom under background CPU/iGPU load and falling well short of the `<= 2.5 s` discrete-GPU target.
- **Citation Verification Bounds:** Deterministic `verify_with_headers` blocks fabricated citations (`0` across all 144 run-questions) and achieved `92.3%` displayed verification, yet `3 / 39` sentences were withheld/unverified because paraphrased 4B phrasing diverged too far from source wording or numeric formatting.

### Extraction Limitations (<StatusBadge status="fail" /> on Blind Held-Out F1)
- **Over-Extraction & False-Positive Precision Noise (`F1 = 0.500` on 58 Blind Items):** While extraction achieved `0.702` F1 on the 22-item `dev` split and `0.996` F1 under oracle inputs, blind held-out evaluation stored **133 facts** (plus `16` held as `needs_review`) against **83 gold facts** — yielding **65.1% recall** but only **40.6% precision**:
  - `action`: `P 0.43`, `R 0.89`, `F1 0.58` (`32 TP`, `42 FP`, `4 FN`)
  - `decision`: `P 0.46`, `R 0.75`, `F1 0.57` (`6 TP`, `7 FP`, `2 FN`)
  - `event`: `P 0.65`, `R 0.92`, `F1 0.76` (`11 TP`, `6 FP`, `1 FN`)
  - `note`: `P 0.12`, `R 0.17`, `F1 0.14` (`3 TP`, `21 FP`, `15 FN`)
  - `question`: `P 0.40`, `R 0.22`, `F1 0.29` (`2 TP`, `3 FP`, `7 FN`)
- **Generalization Gap (`dev` `0.702` vs. `heldout` `0.500`):** Prompt and heuristic refinements tuned on `dev` did not generalize cleanly to unseen screen patterns on the 4B model, generating `42` false-positive `action` items and `21` false-positive `note` items.
- **JSON Reliability & Confidence Limits:** `1 / 58` held-out items failed valid JSON output twice under strict grammar retries; confidence scores remain model-calibrated heuristics rather than calibrated probabilities, and evidence validation guarantees only substring existence (`0.0%` hallucinations), not semantic usefulness.

### Prompt Injection Limitations (<StatusBadge status="pass" /> App Level, Raw Model Still Susceptible)
- **Raw Model vs. Application Guard Separation:** In the `152`-case real-model run (`Qwen3-VL-4B-Instruct-Q4_K_M`), **the raw 4B model still echoed forbidden injected strings in 4 cases**, and the simulated worst-case compromised model echoed them in **130 / 152** (`cases.json`) and **15 / 20** (`unseen_v3.json`).
- **Deterministic Defense Reliance:** Application safety (`0` unsafe stored outcomes across both `152` real cases and `20` `unseen_v3` cases) relies critically on deterministic Rust pre-prompt scrubbing (`scrub_untrusted_segments`), XML tag escaping (`escape_content`), and post-extraction filtering (`tsr-guard::injection` + `tsr-pipeline::guard`). Novel obfuscated phrasings or unsupported non-English scripts outside tested patterns remain an ongoing defense risk.
- **Sidecar Thermal/Timeout Sensitivity:** Running continuous heavy VLM + text injection batches on shared iGPU hardware triggered `3` `model: transport: timed out` failures (`inj-m04`, `s7-055`, `s7-056`) during a back-to-back stress run before passing `152/152` cleanly in an isolated run.

### Draft Limitations (<StatusBadge status="pass" /> Local iGPU Beta / <StatusBadge status="fail" /> Standard)
- **Required Point Coverage & Placeholders:** Although `20 / 20` drafts produced valid schema JSON with `0` unflagged unsupported claims and `20 / 20` avoided forbidden content, **4 / 20 missed at least one `must_include` element** (`16 / 20`) and **1 / 20 missed an expected `[...]` placeholder** (`19 / 20`).
- **Latency & Style Limits:** `5,587 ms` first visible text p50 and `14,175 ms` complete draft p50 satisfy `local-igpu-beta` (`<= 6,000 / 20,000 ms`) but exceed discrete-GPU interactive targets (`<= 3,000 / 8,000 ms`). Style matching clips examples at 320 chars and cannot capture long-form writing voice cleanly on a 4B quant.
- **Application Insertion Scope:** Plain-text insertion, password-control refusal, and clipboard restoration were verified on test controls (`insert_real.rs`), whereas live insertion across **Gmail, Outlook, Teams, Slack, and WhatsApp** and **Windows Win+V history / cloud clipboard suppression** remain <StatusBadge status="partial" /> or <StatusBadge status="not-tested" />.

### Search Limitations (<StatusBadge status="pass" />)
- **FTS5 + `VectorDb` Hybrid Fusion:** SQLite FTS5 BM25 and `VectorDb` (`tessera_vectors.enc`, 384-d `multilingual-e5-small` via ONNX Runtime `1.31.0`) fused via RRF ($k = 60$) now have an executed real-embedder evaluation (`evals/results/search_e5.md`: `R@10 = 1.000`, `MRR = 0.900`, `nDCG@10 = 0.921`).
- **Residual Search Constraints:** Unanswerable questions still retrieve candidates (`8 / 8` hit >= 1 snippet; abstention depends on Ask). Furthermore, forgetting a person via salted-hash suppression filters full-name hits (`7` filtered) but **5 first-name-only mentions** remained searchable (`search_e5.md` note), and simulated-hash CI runs (`sim-hash-v1`) do not exercise semantic E5 quality when `--model` is absent.

---

## 4. Security, Privacy & Platform Limitations

- **Local Mode vs. Optional Groq BYOK:** Tessera defaults strictly to `TSR_LLM_PROVIDER=local` (`llama-server` on loopback `127.0.0.1`). Optional **Groq BYOK** (`openai/gpt-oss-20b`, enabled via `--features groq`) requires explicit user consent and transmits prompts over HTTPS to `api.groq.com`. Screen images (`image_png`) are never sent to Groq (`UnsupportedModality`), and **Groq BYOK quality/latency has NOT been benchmarked live** (<StatusBadge status="not-tested" />).
- **What Can Leave the Device:**
  1. Consented Hugging Face downloads of pinned model/embedder weights in **AI Setup** (verified against `tools/models.lock`).
  2. PII-masked prompt text sent to `api.groq.com` **only** when the user explicitly activates Groq BYOK and supplies an API key.
  3. **Microsoft Edge WebView2 (`msedgewebview2.exe`) platform traffic (`ADR-0002`):** Even with Chromium NetLog showing `0` external URLs (`localhost` only), Windows socket monitoring (`netmon.ps1`) observed **3 TLS port-443 sockets** to Microsoft IP space (`52.110.14.167`, `40.104.66.66`, and `2603:1046:c04::/48` in Stage 0 runs). Elevating `pktmon` for TLS SNI inspection or creating Windows Firewall rules requires Administrator privileges not verified in standard builds.
- **PII Masking Boundaries:** Deterministic regex + Luhn (`[CARD]`) + Verhoeff (`[AADHAAR]`) + contextual pseudonyms (`[PERSON_01]`) achieve `355 / 355` (`0.41%` FP) on synthetic corpora, **not** universal privacy guarantees. Isolated/lowercase names without conversational cues, non-standard street addresses, and garbled OCR strings can pass unmasked.
- **Screen Capture Exclusion Limits:** Win32 `SetWindowDisplayAffinity(..., WDA_EXCLUDEFROMCAPTURE)` (`0x11`) passed synthetic WGC/GDI pixel tests on `2026-10-27`, **failed on `2026-11-28` (`BitBlt: Access is denied. 0x80070005`) while the Windows desktop session was locked/unattended**, and cannot hide windows from kernel-level capture drivers, external cameras, or untested virtual-display hooks.
- **Database Encryption & Windows DPAPI:** SQLCipher (`tessara.db`) and `VectorDb` v2 (`tessera_vectors.enc`, `ring::aead::AES_256_GCM` with 12-byte CSPRNG nonces + AAD) protect data at rest against offline disk theft, but master keys (`key.bin`, `groq_key.bin`) rely on Windows DPAPI (`CryptProtectData`). Any malicious software executing under the same logged-in Windows user account can call `CryptUnprotectData`.
- **Deletion & Retention Scope:** Single-item deletion, automatic retention pruning (`0` = Keep Forever, UI presets `7 / 30 / 90 / 365` days, backend maximum `3650` days), and token-confirmed (`CLEAR`) wipes cascade across SQLCipher tables, FTS5 indexes, `facts`/`evidence`, and `tessera_vectors.enc`, using `PRAGMA secure_delete = ON`. However, prior Windows VSS shadow copies, `pagefile.sys`, or `hiberfil.sys` pages outside BitLocker are not retro-wiped by SQLite.
- **Dependency Advisories Verified Clean (Default + Groq):** `cargo deny --offline --locked check` and `cargo deny --offline --locked --features tessara/groq check` both pass with **0 advisories, 0 bans, and 0 license violations** (`rusqlite 0.43.0`, `sqlcipher-src 420.0.0`, `cookie 0.18.3`, `url 2.5.9`). The optional auto-updater (`--features updater`, `tauri-plugin-updater`) remains **compiled out** (<StatusBadge status="deferred" />) per `ADR-0003` due to transitive `zip` crate advisories (`RUSTSEC-2026-0367`, `-0368`, `-0369`).

---

## 5. Remaining Validation Work (`NOT TESTED` / `PARTIAL`)

| Validation Item | Current State | Status | What Is Required to Close |
|---|---|---|---|
| **Discrete RTX 4050 Benchmarking** | Hardware unavailable; zero measurements taken | <StatusBadge status="not-tested" /> | Execute `tsr-eval` Ask, Extraction, Draft, and Injection suites on RTX 4050 6 GB laptop |
| **Live Groq BYOK Evaluation** | Connectivity probe & `QuotaTracker` unit-tested only; no `evals/results/*groq*` dataset run | <StatusBadge status="not-tested" /> | Run `tsr-eval` suite with `TSR_LLM_PROVIDER=groq` and user API key |
| **Zoom & OBS Capture Exclusion** | Not installed on test machine | <StatusBadge status="not-tested" /> | Live screen-share verification in Zoom and OBS Studio on unlocked desktop |
| **Teams, Snipping Tool & Xbox Game Bar Exclusion** | Packages present (`MSTeams`, `ScreenSketch`, `XboxGamingOverlay`), not verified live during capture | <StatusBadge status="not-tested" /> | Unlocked interactive desktop test capturing active screen-share / screenshot tools |
| **Unlocked Pixel Exclusion Re-Run** | `2026-10-27` passed (`0x11`); `2026-11-28` re-run blocked by `BitBlt 0x80070005` on locked session | <StatusBadge status="partial" /> | Re-run `tools\verify-capture-exclusion.ps1` in an unlocked interactive Windows session |
| **Extended 24h Network & Elevated TLS SNI Audit** | 2-min & 10-min `netmon.ps1` + NetLog completed; `pktmon` & 24h Sysmon/WFP blocked on non-Admin shell | <StatusBadge status="partial" /> | Run 24-hour elevated Sysmon/WFP + `pktmon` capture to classify `msedgewebview2.exe` TLS SNIs (`ADR-0002`) |
| **24h Pagefile & Memory Canary Validation** | 60-second canary scan (`0` plaintext hits) only | <StatusBadge status="partial" /> | Execute 24-hour continuous run and scan disk/pagefile/hiberfil |
| **Live Mail/Chat Drafting & Win+V History** | Plain edit control tested (`insert_real.rs`); Gmail/Outlook/Teams/Slack/WhatsApp untested; Win+V history disabled | <StatusBadge status="partial" /> | Test live insertion in target desktop/web apps with Windows Clipboard History enabled |
| **2-Week Dogfood Acceptance & 7-Day Stability** | MVP quality & Stage 8 audio/translation gates unclosed | <StatusBadge status="not-tested" /> | Track real daily draft acceptance ($\ge 50\%$) and 7-day continuous uptime after extraction F1 recovery |
