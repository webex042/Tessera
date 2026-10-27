# Task ledger: Stages 3–10

Status labels: **PASS** (done and verified with evidence), **PARTIAL** (done in part, or verified
only in part), **BLOCKED** (needs an owner action, approval, account, hardware or download),
**FAIL** (measured and missed), **NOT STARTED**. Evidence links point to committed files.

Last updated: 2026-10-27.

## Stage 2 carry-over (real model)

| id | task | status | evidence / blocker |
|---|---|---|---|
| 2.R1 | Download Qwen3-VL-4B Q4_K_M + mmproj (approved), verify digests | PASS | `tsr-models locate --verify`: both files `digest_checked: true` |
| 2.R2 | Held-out extraction with the real model | FAIL | F1 0.507 (< 0.70); hallucinated 0 %; 0 invalid rows; 15/58 items failed on invalid JSON twice. `evals/results/extraction_real_heldout.md` |
| 2.R3 | Injection corpus with the real model | PARTIAL | first run 66/74 (5 safety failures) → guard fixed → 70/74, all 69 adversarial pass; 4 genuine-task misses. `evals/results/injection_real.md` |

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
| 4.E3 | 7-day replay: active hours only | PASS | SIMULATED model + code-only; real-model run pending |
| 4.E4 | No auto-close | PASS | `no_auto_close.rs`, `loops_brief.rs` |

## Stage 5: Draft assistant ([verification](stage5/verification.md))

| id | task | status | evidence / blocker |
|---|---|---|---|
| 5.1 | Focus target, editability, password checks | PASS | `insert_real.rs` (REAL) |
| 5.2 | Thread context, memory, P0 streaming draft | PASS | `tsr-draft` tests; app wiring builds; real model eval pending |
| 5.3 | Code fact check | PASS | `factcheck` tests; SIMULATED 0 unflagged |
| 5.4 | Private clipboard insertion, restore, fallbacks, tokens | PARTIAL | REAL insert + restore PASS; Win+V history BLOCKED (history off); cloud upload not observable |
| 5.E1 | Works in Gmail/Outlook/Teams/Slack/WhatsApp + plain field | PARTIAL | plain field REAL; named apps need the owner |
| 5.E2 | Never into password / changed target | PASS | `insert_real.rs` |
| 5.E3 | Clipboard restored; not in history / cloud | PARTIAL | restore PASS; history BLOCKED; cloud not observable |
| 5.E4 | First text ≤ 3 s p50; complete ≤ 8 s p50 | NOT STARTED | real-model run in progress |
| 5.E5 | 0 unflagged unsupported claims (20 scenarios) | PARTIAL | SIMULATED 0; real-model run in progress |

## Stage 6: Ask ([verification](stage6/verification.md))

| id | task | status | evidence / blocker |
|---|---|---|---|
| 6.1 | Query filters, retrieval, prompt, streaming, verification, chips, insufficient path | PASS | `askq`/`ask` tests; SIMULATED eval PASS; preview |
| 6.E1 | ≥ 80 % correct, ≥ 90 % verified, 0 fabricated (60 q) | PARTIAL | SIMULATED 88.3 % / 100 % / 0; real-model run in progress |
| 6.E2 | First token ≤ 2.5 s, full ≤ 10 s | NOT STARTED | real-model run in progress |

## Stages 7–10

| stage | status | note |
|---|---|---|
| 7 Security audit | NOT STARTED | |
| 8 Meetings and translation | NOT STARTED | whisper.cpp / Opus-MT model downloads need approval |
| 9 Closed beta diagnostics | NOT STARTED | |
| 10 Release engineering | NOT STARTED | signing needs the owner's certificate |
