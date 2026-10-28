# Final report: Stages 3–10 (2026-10-27/28)

Status labels: PASS / PARTIAL / BLOCKED / FAIL / NOT STARTED. Per-task detail: [`ledger.md`](ledger.md).
Machine: Windows 11, AMD Radeon integrated GPU (Vulkan), Qwen3-VL-4B-Instruct Q4_K_M + mmproj
(downloaded with approval, digests verified). The plan's target machine (RTX 4050, 6 GB) was not
available; every timing below is from the integrated GPU.

## A. Summary

| Stage | Status | One line |
|---|---|---|
| 2 (carry-over) | FAIL / PARTIAL | real model: extraction F1 0.533 (FAIL, was 0.507; invalid-JSON failures fixed by ADR-0010), hallucination 0 % (PASS), injection safety holds after guard fixes |
| 3 Notch UI | PARTIAL | built; 60 fps, input-to-visual p95 64 ms, focus and capture exclusion verified; named apps and live monitor/DPI change need the owner |
| 4 Rollups, brief, loops | PARTIAL | built; replay, drill-down, no-auto-close, brief timing, real-model rollups PASS; unlock trigger not exercised live |
| 5 Drafts | PARTIAL | built; insertion safety PASS (real); 0 unflagged claims (real); timings FAIL on the iGPU; Win+V history untestable here (off) |
| 6 Ask | FAIL | built; pipeline PASS (simulated); real model 66.7 % correct, 83.7 % verified, 0 fabricated after the key-order fix (was 15 %); 25 s on the iGPU |
| 7 Security audit | PARTIAL | corpora, robustness, canary, deny, Tauri, integrity, docs done; S1 FAIL (WebView2), S3/S4 FAIL; 24 h runs need the owner |
| 8 Meetings | BLOCKED | MVP gate not met; model downloads not approved |
| 9 Beta | PARTIAL | diagnostics bundle PASS; beta needs testers |
| 10 Release | PARTIAL | SBOM, NSIS config, signing guide, updater gate closed; build and signing need the owner |

## B. What was built

- **Stage 3:** fixed-size notch window with per-panel regions; Rust-placed opens; focus return;
  pill/peek/expanded/draft; Today, Open loops, Search, Timeline, Ask, Diagnostics views; typed IPC;
  hotkeys with fallback chords; tray; keyboard navigation, reduced motion, forced colours, themes;
  capture exclusion on every window with a pixel verifier; perf/focus harness; synthetic seeding;
  dev-only IPC mock for browser previews.
- **Stage 4:** summaries + provenance (migration 0004), hour→month rollups with map-reduce,
  validation and code-only fallback, staleness trigger, P2 runner; code-assembled brief with
  unlock/start trigger; open-loop groups, proposed-done from completion evidence, dismiss-all
  with a bulk token; Timeline week/month with drill-down.
- **Stage 5:** `tsr-draft` (request, streaming view, strict output, fact check), UIA focus target,
  private clipboard with all-format save/restore, Ctrl+V into the re-verified field, insert/copy
  tokens, draft panel with flags and an injected-thread banner, content-free audit events.
- **Stage 6:** past-oriented query filters, keyword+facts retrieval, snippets, streaming sentence
  parser, per-sentence citation verification, Ask tab with chips, ask/v2 prompt.
- **Stage 7:** 152-case injection corpus (78 held-out, written first), 355-case secrets corpus,
  new detectors, guard rules, randomised robustness tests, canary scan, network run, cargo deny,
  licence inventory, Tauri security tests, model digest stamp, SECURITY.md, privacy.md.
- **Stage 9:** content-free diagnostics bundle, shown before saving.
- **Stage 10:** NSIS configuration (inactive), CycloneDX SBOM generator and SBOM, signing guide.

## C. Bugs found and fixed by measurement

Second WGC capture crashed the process; default hotkey owned by another app; focus stuck in the
hidden notch; input latency p95 292 → 64 ms; clipboard never restored (sequence number) — **the
dev machine's clipboard content was lost in that test**; text-only clipboard saving refused most
clipboards; a completion for one person proposed another's loop; dedupe swallowed completion
evidence; real model copied an injected request into a draft; five injection guard gaps on the
real model; date-resolver panic; non-idempotent redaction; Aadhaar VID/passport/voter ID not
detected; schema keys sent to the model in alphabetical order (ADR-0010); the exfiltration rule missed gerunds ("by sharing your OTP", real model); model files not digest-checked at start; two timing-fragile scheduler tests.

## D. Exit-criteria scorecard

| Criterion | Measured | Status |
|---|---|---|
| S3 Notch absent from captures | WGC + GDI pixels: 100 % absent (pill, expanded); named apps not run | PARTIAL |
| S3 60 fps; input-to-visual ≤ 100 ms | frame p95 16.8 ms; p95 64 ms | PASS |
| S3 No focus stealing | foreground unchanged during a peek; 20/20 focus return | PASS |
| S3 Placement after monitor/DPI change | unit-tested only | PARTIAL |
| S4 Highlights resolve; drill-down | tests + real-model rollups (0 invalid ids) | PASS |
| S4 Brief ≤ 30 s after unlock | 7 ms (start-up trigger) | PARTIAL |
| S4 7-day replay | exact active hours; real model 15/15 | PASS |
| S4 No auto-close | tests + source scan | PASS |
| S5 Works in Gmail/Outlook/Teams/Slack/WhatsApp + plain field | plain field REAL | PARTIAL |
| S5 Never into password / changed target | REAL test | PASS |
| S5 Clipboard restored; not in history/cloud | restore REAL; history off; cloud not observable | PARTIAL |
| S5 First text ≤ 3 s; complete ≤ 8 s | 5.9 s / 16.4 s (iGPU) | FAIL |
| S5 0 unflagged unsupported claims | real 0 | PASS |
| S6 ≥ 80 % correct, ≥ 90 % verified, 0 fabricated | real: 66.7 % / 83.7 % / 0 (after ADR-0010; 15 % before) | FAIL |
| S6 First token ≤ 2.5 s, full ≤ 10 s | 25.2 s / 25.5 s (iGPU) | FAIL |
| MVP S1 network (24 h) | 10 min: Tessara 0, WebView2 1 HTTPS (ADR-0002) | FAIL |
| MVP S2 canary (24 h) | 60 s: 0 hits, control found | PARTIAL |
| MVP S3 extraction F1 ≥ 0.70, halluc. ≤ 3 % | 0.533 / 0 % (after ADR-0010; was 0.507) | FAIL |
| MVP S4 Ask verified ≥ 90 %, 0 fabricated | 83.7 % verified, 0 fabricated | FAIL |
| MVP S5 drafts accepted ≥ 50 % (2-week self-test) | not run (needs the owner) | BLOCKED |
| MVP S6 resources on RTX 4050 | hardware not available | BLOCKED |
| MVP S7 7-day stability | not run | BLOCKED |

## E. Real-model results (Qwen3-VL-4B Q4_K_M, Vulkan iGPU)

See `evals/results/*_real*`: extraction F1 0.533 after the key-order fix (ADR-0010) — the fix
removed all 15/58 invalid-JSON failures (recall 44.6 %→57.8 %) but F1 still misses 0.70 because
the model over-produces low-value `note` facts (precision 58.7 %→49.5 %); sorted-key run kept as
`extraction_real_heldout_sortedkeys.*`; injection 66→70/74 on the original set; 152-case run 123/152 (held-out half 24/39 — 12 injected
texts stored as to-dos, none executed; 5 request timeouts in a run paused twice by standby);
drafts 17/20 valid, 0 unflagged, slow; rollups PASS; Ask 15 % → 66.7 % once schemas kept their key
order (ADR-0010: every schema had been sent to the model with its keys sorted alphabetically,
so Ask had to write "insufficient" before any answer and extraction wrote `confidence` before
the fact). All results before that fix were measured with sorted keys.

## F. Security and privacy

`SECURITY.md`, `docs/privacy.md`, `docs/stage7/verification.md`. Open: ADR-0002 (WebView2
connection), ADR-0003 (updater advisories), held-out injection generalisation 69 % against a
fully compromised model (SIMULATED) and 62 % with the real 4B model (facts are never executed, which is the backstop).

## G. Blocked — what needs the owner

1. Approve tool downloads: cargo-audit, nightly + cargo-fuzz, cargo-cyclonedx (optional), NSIS toolchain (`tauri build`), and a fresh RustSec DB / `pnpm audit` (network).
2. Run the 24-hour network (Sysmon/WFP, elevated) and canary tests; turn on clipboard history for the Win+V test.
3. Live checks: Teams/Zoom/OBS/Snipping Tool/Game Bar captures; monitor plug/DPI change; lock/unlock for the brief; drafts in Gmail/Outlook/Teams/Slack/WhatsApp.
4. Decide on ADR-0002 (WebView2 connection) and supply a code-signing certificate.
5. Approve whisper.cpp / Opus-MT downloads (Stage 8) once the MVP gate is met.
6. Model quality on this hardware: decide whether to try Q5_K_M / 8B on the RTX 4050 machine, or prompt/schema changes (each needs a new prompt version and a full eval).

## H. Next steps (recommended order)

1. Re-run drafts and rollups with declared key order (ADR-0010) for completeness (extraction already re-run: F1 0.533, FAIL). To lift extraction F1, curb low-value `note` facts (precision driver) rather than only the JSON format.
2. Re-run held-out extraction and Ask after any prompt change; on the RTX 4050 if possible.
3. Author a third, untouched injection set before any further guard tuning.
4. Owner checklist items in G.
