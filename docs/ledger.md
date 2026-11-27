# Task ledger: Stages 3–10

Status labels: **PASS** (done and verified with evidence), **PARTIAL** (done in part, or verified
only in part), **BLOCKED** (needs an owner action, approval, account, hardware or download),
**FAIL** (measured and missed), **NOT STARTED**. Evidence links point to committed files.

Last updated: 2026-11-27.

> **Benchmark Profiles (`evals/runner/src/profile.rs`):** All evaluations now distinguish the
> **Standard Product Profile** (`--profile standard`, RTX 4050 6 GB target — hardware unavailable)
> from the **Local iGPU Beta Profile** (`--profile local-igpu-beta`, `Qwen3-VL-4B-Instruct-Q4_K_M`
> on AMD Radeon Integrated Graphics via Vulkan). Security targets (100 % injection safety, 100 %
> secret redaction, 0 unauthorized application network connections) are non-negotiable across both
> profiles. Unified comparative output: `evals/results/scorecard.md`.

## Stage 2 carry-over (real model)

| id | task | status | evidence / blocker |
|---|---|---|---|
| 2.R1 | Download Qwen3-VL-4B Q4_K_M + mmproj (approved), verify digests | PASS | `tsr-models locate --verify`: both files `digest_checked: true` |
| 2.R2 | Held-out extraction with the real model | FAIL | Standard target `≥ 0.70`, Local iGPU Beta target `≥ 0.60` (provisional `≥ 0.50` rejected due to 1 SE margin and 49.5 % precision noise). Measured after ADR-0010: **F1 0.533** (`0 %` hallucinated, `0` schema-invalid, P 49.5 %, R 57.8 %; was F1 0.507 / 15 invalid JSON). `evals/results/extraction_real_heldout.md` |
| 2.R3 | Injection corpus with the real model | PARTIAL | Non-negotiable target: `152/152` safe. Historical real 4B run: `123/152` (`24/39` held-out: `13` safety leaks, `10` utility/kind misses, `1` extra open fact, `5` standby timeouts). Deterministic code guard now passes `152/152` SIMULATED + `13/13` real-model safety replay (`crates/tsr-pipeline/tests/injection_real_replay.rs`), with prompt `<thread>` escaping fixed; live hardware re-run + 3rd unseen set still pending |

## Stage 3: Notch UI ([verification](stage3/verification.md))

| id | task | status | evidence / blocker |
|---|---|---|---|
| 3.1 | Window mechanics | PASS | `window.rs` tests; `notch_perf.json` |
| 3.2 | Capture exclusion + verifier | PASS | `capture_exclusion_{pill,expanded}.json` |
| 3.3 | Shells and views on real data | PASS | `src-tauri/tests/views.rs`; preview |
| 3.4 | Hotkeys, tray, keyboard, reduced motion, high contrast, themes | PARTIAL | hotkeys REAL; reduced motion / forced colours not exercised with the Windows settings switched |
| 3.5 | Typed IPC, error codes, states | PASS | `ui/src/ipc/*`, `views.rs` |
| 3.E1 | Absent from Teams/Zoom/OBS/Snipping Tool/Game Bar | PARTIAL | WGC + GDI pixels PASS; apps need the owner |
| 3.E2 | 60 fps; input-to-visual ≤ 100 ms | PASS | frame p95 16.8 ms; p95 64 ms |
| 3.E3 | No focus stealing during peeks | PASS | `notch_perf.json` |
| 3.E4 | Placement after monitor/DPI change | PARTIAL | unit-tested; live change needs the owner |

## Stage 4: Rollups, brief, open loops ([verification](stage4/verification.md))

| id | task | status | evidence / blocker |
|---|---|---|---|
| 4.1 | Rollups with provenance, staleness, regeneration | PASS | `tests/rollup.rs` (8 tests) |
| 4.2 | Brief assembly + unlock trigger | PARTIAL | assembly + start-up trigger REAL (7 ms); unlock not exercised live |
| 4.3 | Open loops, proposed-done, stale grouping, dismiss-all | PASS | `tests/loops_brief.rs`, `views.rs` |
| 4.4 | Timeline week/month | PASS | `views.rs` test; preview |
| 4.E1 | Highlight ids resolve; drill-down month → item | PASS | rollup + views tests |
| 4.E2 | Brief ≤ 30 s after first unlock | PARTIAL | 7 ms at start-up; unlock path not live-tested |
| 4.E3 | 7-day replay: active hours only | PASS | SIMULATED + code-only; REAL model rollups: 11/11 active hours, 15/15 periods model-written, 0 invalid ids (`evals/results/rollup_real.md`) |
| 4.E4 | No auto-close | PASS | `no_auto_close.rs`, `loops_brief.rs` |

## Stage 5: Draft assistant ([verification](stage5/verification.md))

| id | task | status | evidence / blocker |
|---|---|---|---|
| 5.1 | Focus target, editability, password checks | PASS | `insert_real.rs` (REAL) |
| 5.2 | Thread context, memory, P0 streaming draft | PASS | `tsr-draft` tests; REAL: 17/20 valid drafts (2 hit the token limit, 1 timed out) |
| 5.3 | Code fact check | PASS | `factcheck` tests; SIMULATED 0 unflagged |
| 5.4 | Private clipboard insertion, restore, fallbacks, tokens | PARTIAL | REAL insert + restore PASS; Win+V history BLOCKED (history off); cloud upload not observable |
| 5.E1 | Works in Gmail/Outlook/Teams/Slack/WhatsApp + plain field | PARTIAL | plain field REAL; named apps need the owner |
| 5.E2 | Never into password / changed target | PASS | `insert_real.rs` |
| 5.E3 | Clipboard restored; not in history / cloud | PARTIAL | restore PASS; history BLOCKED; cloud not observable |
| 5.E4 | First text ≤ 3 s p50; complete ≤ 8 s p50 | FAIL | REAL on the AMD iGPU: 5.9 s / 16.4 s (`evals/results/draft_real_all.md`); target hardware (RTX 4050) not available |
| 5.E5 | 0 unflagged unsupported claims (20 scenarios) | PASS | REAL: 0 unflagged; one draft echoed an injected request (dr-017) — now flagged + banner (ADR-0009) |

## Stage 6: Ask ([verification](stage6/verification.md))

| id | task | status | evidence / blocker |
|---|---|---|---|
| 6.1 | Query filters, retrieval, prompt, streaming, verification, chips, insufficient path | PASS | `askq`/`ask` tests; SIMULATED eval PASS; preview |
| 6.E1 | ≥ 80 % correct, ≥ 90 % verified, 0 fabricated (60 q) | FAIL | Standard `≥ 80 % / ≥ 90 %`; Local iGPU Beta `≥ 70 % (blind) / ≥ 85 %` (provisional `≥ 65 %` rejected: 95 % Wilson CI `[54.1 %, 77.3 %]` and `ask/v2` lacked a blind split). Measured REAL `ask/v2`: **66.7 % correct (40/60), 83.7 % verified (41/49), 0 fabricated (`PASS`), 0 invalid**. Before ADR-0010: 15 % |
| 6.E2 | First token ≤ 2.5 s, full ≤ 10 s | FAIL | Standard `≤ 2.5 s / ≤ 10 s`, Local iGPU Beta `≤ 15 s / ≤ 30 s`. Measured REAL (iGPU): first sentence p50 **25.2 s** (`FAIL`), full **25.5 s** (`≤ 30 s`); RTX 4050 not available |

## Stage 7: Security audit ([verification](stage7/verification.md))

| id | task | status | evidence / blocker |
|---|---|---|---|
| 7.1 | Threat-model review, gaps fixed | PASS | 6 gaps fixed (ADR-0009) + `<thread>`/`<style_examples>` delimiter escaping and `rollup/v1` hierarchy fix |
| 7.2 | Injection ≥ 150, secrets ≥ 300 incl. Indian IDs, in CI | PARTIAL | **152/152** SIMULATED + **13/13** real-model safety replay PASS (after `s7-002`, `s7-018`, `s7-050` fixes); secrets **355/355** redacted, FP **0.41 %** (`PASS`). Historical live 4B run remains **123/152** (`24/39` held-out) until re-run on hardware + 3rd unseen corpus |
| 7.3 | Fuzzing | PARTIAL | randomised robustness tests (2 bugs found and fixed); cargo-fuzz BLOCKED (nightly + downloads need approval) |
| 7.4 | 24 h network monitor | FAIL | 10-min run: Application (`tessara.exe`, `llama-server.exe`) **0** outbound connections (`PASS`); WebView2 (`msedgewebview2.exe`) **1** HTTPS connection to Microsoft (`2603:1046:c04::/48:443`, ADR-0002 `FAIL`); 24 h Sysmon/WFP + TLS SNI capture needs the owner |
| 7.5 | Canary disk scan | PARTIAL | 0 plaintext hits, control found (60 s run); 24 h and pagefile need the owner |
| 7.6 | cargo-deny / cargo-audit / pnpm audit, notices | PARTIAL | deny all ok (cached advisory DB); cargo-audit, pnpm audit, DB refresh BLOCKED (network); THIRD_PARTY_NOTICES.md PASS |
| 7.7 | Tauri review | PASS | `security_config.rs` |
| 7.8 | Sidecar/model integrity | PASS | model digest stamp; sidecar manifest |
| 7.9 | privacy.md, SECURITY.md | PASS | rewritten with evidence |
| 7.E1 | MVP criteria S1–S7 | FAIL | S1 FAIL (WebView2), S2 PARTIAL (60 s), S3 FAIL (F1 0.507), S4 FAIL, S5 PARTIAL (no 2-week self-test), S6 not measured (needs RTX 4050), S7 not run (7 days) |
| 7.E2 | All high/critical findings fixed | PARTIAL | all found in code fixed; ADR-0002 (WebView2 connection) open |
| 7.E3 | README privacy claims match tests | PASS | README updated to the tested state |

## Stages 8–10

| id | task | status | note |
|---|---|---|---|
| 8 | Meetings and translation | BLOCKED | plan gates Stages 8–10 on all MVP criteria passing; whisper.cpp / Opus-MT downloads not approved |
| 9.1 | Content-free diagnostics bundle, reviewed before saving | PASS | `diagnostics.rs`, privacy test |
| 9.2 | Closed beta with testers | BLOCKED | needs testers |
| 10.1 | NSIS installer | BLOCKED | configured (`bundle.active=false`); toolchain download needs approval |
| 10.2 | Signing | BLOCKED | needs the owner's certificate; guide in `docs/release/README.md` |
| 10.3 | CycloneDX SBOM | PASS | `tools/gen_sbom.py`, `docs/release/sbom.cdx.json` (348 components) |
| 10.4 | Updater gate kept closed | PASS | still fails on RUSTSEC-2026-0367/-0368/-0369; feature off |
