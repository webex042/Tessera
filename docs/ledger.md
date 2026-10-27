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
| 2.R3 | Injection corpus with the real model | NOT STARTED | running at time of writing |

## Stage 3: Notch UI ([verification](stage3/verification.md))

| id | task | status | evidence / blocker |
|---|---|---|---|
| 3.1 | Window mechanics (transparent, top-centre, DPI, multi-monitor, topmost, no focus steal) | PASS | `window.rs` tests; `notch_perf.json` (focus 20/20, no steal at start) |
| 3.2 | Capture exclusion + `tools/verify-capture-exclusion.ps1` (WGC whole monitor) | PASS | `capture_exclusion_{pill,expanded}.json` |
| 3.3 | Pill / peek / expanded / draft; Today, Open loops, Search, Timeline on real data | PASS | `src-tauri/tests/views.rs`; browser preview on synthetic data |
| 3.4 | Hotkeys, tray, keyboard nav, reduced motion, high contrast, light/dark | PARTIAL | hotkeys REAL (`hotkey_real.rs`, 20/20 opens); light/dark seen in preview; reduced motion and forced colours implemented in CSS but not exercised with the Windows settings switched |
| 3.5 | Typed IPC, error codes, loading/empty/error states | PASS | `ui/src/ipc/*`, `views.rs` error codes, `Status` component |
| 3.E1 | Exit: absent from Teams, Zoom, OBS, Snipping Tool, Game Bar | PARTIAL | API level PASS (WGC + GDI); the apps themselves need the owner (checklist §D) |
| 3.E2 | Exit: 60 fps; input-to-visual ≤ 100 ms | PASS | p95 frame 16.8 ms; input-to-visual p95 64 ms |
| 3.E3 | Exit: no focus stealing during peeks | PASS | foreground unchanged while a peek showed |
| 3.E4 | Exit: placement after monitor plug/unplug and DPI change | PARTIAL | maths unit-tested; live change needs the owner |

## Stages 4–10

| stage | status | note |
|---|---|---|
| 4 Rollups, brief, open loops | NOT STARTED | |
| 5 Draft assistant | NOT STARTED | |
| 6 Ask with citations | NOT STARTED | |
| 7 Security audit | NOT STARTED | |
| 8 Meetings and translation | NOT STARTED | whisper.cpp / Opus-MT model downloads need approval |
| 9 Closed beta diagnostics | NOT STARTED | |
| 10 Release engineering | NOT STARTED | signing needs the owner's certificate |
