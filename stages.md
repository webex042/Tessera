# Tessara — Stages

> Stage goals, exit criteria and main risks. Companion to [`implementation_plan.md`](implementation_plan.md), which has the full task lists (§17).
> Where the two disagree, update both in the same PR.

Effort is in full-time-equivalent weeks for one developer; double it for part-time. MVP (Stages 0–7) ≈ 26 FTE-weeks. Stages 8–10 start only after every MVP success criterion (plan §1.3) passes.

| Stage | Name | Effort | Depends on | Status |
|---|---|---|---|---|
| 0 | Feasibility and model benchmark | 2 wk | — | Not started |
| 1 | Foundation, capture, memory, baseline guardrails | 5 wk | 0 | Not started |
| 2 | Understanding pipeline | 4 wk | 1 | Not started |
| 3 | Notch UI | 3 wk | 1 | Not started |
| 4 | Rollups, morning brief, open loops | 3 wk | 2, 3 | Not started |
| 5 | Draft assistant | 3 wk | 2, 3 | Not started |
| 6 | Ask with sources | 2 wk | 2 | Not started |
| 7 | Security hardening and guardrail audit | 4 wk | 4, 5, 6 | Not started |
| 8 | Meeting notes and live translation | ~5 wk | 7 | Post-MVP |
| 9 | Closed beta | ~4 wk | 7 | Post-MVP |
| 10 | Release engineering | ~3 wk | 9 | Post-MVP |

Stage 3 can run in parallel with Stage 2 if there are two contributors.

---

## Stage 0 — Feasibility and model benchmark

**Goal:** prove the 4B model, capture stack and hardware budget before building the app.

**Exit criteria (go / no-go):**
- [ ] VRAM peak ≤ 5.2 GB with vision at 8 k ctx on the RTX 4050 6 GB.
- [ ] Extraction: JSON valid ≥ 99 %, task/commitment F1 ≥ 0.65, p50 latency ≤ 8 s for a 1 500-token item.
- [ ] Ask: p50 time to first token ≤ 2.5 s with 1 500 tokens of snippets.
- [ ] Draft: blind self-rating ≥ 3/5 average on the 20 scenarios.
- [ ] UIA or OCR yields usable text for ≥ 16 of the top 20 apps.

**If it fails:** Q5_K_M or 8B on bigger GPUs; split models (text-only 4B + VLM only for rung 3); 4 k ctx; narrow v1 to search + brief.

**Also measured:** 2 h WebView2 network spike (plan §13.5); `cargo deny check bans` dependency spike with `fastembed`/`ort` at `default-features = false` (plan §10.1).

**Main risks:** 4B quality too low; WebView2 transparency/animation; WGC yellow border; WebView2 background network traffic; default crate features pulling in network code.

## Stage 1 — Foundation, capture, memory, baseline guardrails

**Exit criteria:**
- [ ] 8-hour run: no crash, background CPU ≤ 3 %, app RAM ≤ 250 MB.
- [ ] No image files ever written (Process Monitor + FS-watcher test on data dir and `%TEMP%`).
- [ ] Blocklisted apps and private windows produce zero items.
- [ ] Title rules don't over-block: ordinary non-browser windows titled with "login", "sign in" or "password" are still captured.
- [ ] Password text from a test WinForms/WPF app never appears in items.
- [ ] Redaction corpus: 100 % of seeded secrets redacted; false positives ≤ 2 % on the clean corpus.
- [ ] DB unreadable without the key; no canary strings in a hex scan.

**Main risks:** UIA coverage gaps; vendored OpenSSL build friction; private-browsing detection misses.

## Stage 2 — Understanding pipeline

**Exit criteria:**
- [ ] Extraction F1 ≥ 0.70 on the held-out set; hallucinated facts ≤ 3 %; 0 schema-invalid rows stored.
- [ ] Injection corpus (≥ 50 cases): no injected instruction stored as a user task (unless genuinely addressed to the user); nothing triggers an action.
- [ ] A 2-hour synthetic workload is fully processed within 30 min after it ends.
- [ ] GPU idle when nothing is queued; model unloads after the idle timeout on battery.

**Main risks:** prompt injection producing misleading memory; llama.cpp flag/API changes; VRAM contention.

## Stage 3 — Notch UI

**Exit criteria:**
- [ ] Notch absent from Teams, Zoom, OBS, Snipping Tool and Xbox Game Bar captures.
- [ ] Expand/collapse at 60 fps; input-to-visual ≤ 100 ms.
- [ ] No focus stealing during peeks.
- [ ] Correct placement after monitor plug/unplug and DPI change.

**Main risks:** WebView2 transparency glitches (fallback: native notch, plan D1).

## Stage 4 — Rollups, morning brief, open loops

**Exit criteria:**
- [ ] Every highlight's IDs exist and resolve to items; drill-down works from month to item.
- [ ] Brief ready ≤ 30 s after first unlock (or pre-computed overnight on AC).
- [ ] 7-day synthetic replay: rollups for every active hour, none for idle hours.
- [ ] No fact auto-closed without user confirmation.

## Stage 5 — Draft assistant

**Exit criteria:**
- [ ] Works in Gmail web, Outlook (new), Teams, Slack, WhatsApp Desktop and a plain text field.
- [ ] Never inserts into a password field or a changed target.
- [ ] Clipboard restored after insertion; draft never appears in Win+V history or cloud clipboard.
- [ ] First visible draft text ≤ 3 s p50; complete ≤ 8 s p50.
- [ ] 0 unsupported dates/amounts/names shown without a flag on the 20-scenario set.

**Main risks:** antivirus flagging clipboard + `SendInput` behaviour; app-specific paste quirks.

## Stage 6 — Ask with sources

**Exit criteria:**
- [ ] 60-question set: ≥ 80 % answered correctly; ≥ 90 % of shown sentences verified; 0 fabricated citations.
- [ ] p50 first token ≤ 2.5 s; full answer ≤ 10 s.

## Stage 7 — Security hardening and guardrail audit

**Exit criteria:**
- [ ] MVP success criteria S1–S7 (plan §1.3) all met.
- [ ] All high/critical findings fixed.
- [ ] README "Privacy model" claims match tested behaviour.

**Main risks:** network test failing on third-party components (WebView2, updater); dependency licence surprises.

## Stages 8–10 (post-MVP, outline)

- **8 — Meetings and translation:** WASAPI audio in RAM only, per-meeting consent, whisper.cpp STT, Opus-MT translation (NLLB is non-commercial).
- **9 — Closed beta:** 10–20 testers on varied hardware; content-free diagnostics bundle the tester reviews before sending.
- **10 — Release:** signed NSIS installer and sidecar, opt-in signed updater (behind the `updater` feature, the one network exception outside `tsr-net`), SBOM per release.

See plan §17 for details.
