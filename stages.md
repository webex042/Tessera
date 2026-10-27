# Tessara — Stages

> Stage goals, exit criteria and main risks. Companion to [`implementation_plan.md`](implementation_plan.md), which has the full task lists (§17).
> Where the two disagree, update both in the same PR.

Effort is in full-time-equivalent weeks for one developer; double it for part-time. MVP (Stages 0–7) ≈ 26 FTE-weeks. Stages 8–10 start only after every MVP success criterion (plan §1.3) passes.

| Stage | Name | Effort | Depends on | Status |
|---|---|---|---|---|
| 0 | Feasibility and model benchmark | 2 wk | — | **In progress** — tooling done; model gates unverified; S1 WebView2 finding open ([results](docs/stage0/results.md)) |
| 1 | Foundation, capture, memory, baseline guardrails | 5 wk | 0 | **In progress** — store, logging, blocklist, redaction, UIA/OCR rungs, OS events + gates, capture engine + sessions + store writer done (real end-to-end capture verified); UI shell, pause hotkey, onboarding, image-write and password-box fixture tests, 8 h run pending |
| 2 | Understanding pipeline | 4 wk | 1 | **In progress** — real model (Qwen3-VL-4B Q4_K_M, Vulkan iGPU): F1 0.507 **FAIL**, hallucination 0 % PASS, injection 70/74 with all adversarial cases passing after guard fixes ([verification](docs/stage2/verification.md)) |
| 3 | Notch UI | 3 wk | 1 | **In progress** — all tasks built; 60 fps, input-to-visual p95 64 ms, no focus steal and capture exclusion (WGC + GDI pixels) verified; named-app captures and live monitor/DPI change need the owner ([verification](docs/stage3/verification.md)) |
| 4 | Rollups, morning brief, open loops | 3 wk | 2, 3 | **In progress** — built; replay, drill-down, no-auto-close and brief timing verified; unlock trigger and real-model rollups pending ([verification](docs/stage4/verification.md)) |
| 5 | Draft assistant | 3 wk | 2, 3 | **In progress** — built; REAL insertion, password/changed-target refusal and clipboard restore verified; named apps, Win+V history (off here) and real-model timings pending ([verification](docs/stage5/verification.md)) |
| 6 | Ask with sources | 2 wk | 2 | **In progress** — built; SIMULATED eval passes (88 % correct, 100 % verified, 0 fabricated); real-model eval pending ([verification](docs/stage6/verification.md)) |
| 7 | Security hardening and guardrail audit | 4 wk | 4, 5, 6 | **In progress** — corpora (152 injection / 355 secrets), robustness tests, canary scan, deny/licences, Tauri review, model integrity, privacy/security docs done; S1 fails on the WebView2 connection, S3/S4 fail with the 4B model on this iGPU; 24 h runs need the owner ([verification](docs/stage7/verification.md)) |
| 8 | Meeting notes and live translation | ~5 wk | 7 | **Blocked** — MVP gate not met; model downloads not approved (ADR-0009) |
| 9 | Closed beta | ~4 wk | 7 | Post-MVP — content-free diagnostics bundle built; beta needs testers |
| 10 | Release engineering | ~3 wk | 9 | Post-MVP — NSIS config, SBOM generator, signing guide; installer build and signing need the owner ([release](docs/release/README.md)) |

Stage 3 can run in parallel with Stage 2 if there are two contributors.

---

## Stage 0 — Feasibility and model benchmark

**Goal:** prove the 4B model, capture stack and hardware budget before building the app.

**Exit criteria (go / no-go):** (status 2026-10-26 in brackets; details in `docs/stage0/results.md`)
- [ ] GPU memory peak ≤ 5.2 GB with vision at 8 k ctx on the 6 GB reference GPU; other GPUs: chosen offload fits reported free memory. *(unverified: no model; detection/selection works on the real AMD iGPU — `docs/hardware-backends.md`)*
- [ ] Extraction: JSON valid ≥ 99 %, task/commitment F1 ≥ 0.65, p50 latency ≤ 8 s for a 1 500-token item. *(unverified; harness + 80-item eval set ready)*
- [ ] Ask: p50 time to first token ≤ 2.5 s with 1 500 tokens of snippets. *(unverified; 60-question set ready)*
- [ ] Draft: blind self-rating ≥ 3/5 average on the 20 scenarios. *(unverified; 20 scenarios ready)*
- [ ] UIA or OCR yields usable text for ≥ 16 of the top 20 apps. *(7 tested, 7 usable; 13 not installed or need the owner's accounts)*
- [ ] WebView2 network spike: no unexpected outbound connection over 2 h. *(**fails** in 10/5/4-min runs: one Microsoft 443 connection from WebView2 — ADR-0002)*
- [x] Dependency spike: no network-capable crate reachable outside `tsr-net`, default and `updater` builds. *(passes; checker self-tested on bad fixtures)*

**If it fails:** Q5_K_M or 8B on bigger GPUs; split models (text-only 4B + VLM only for rung 3); 4 k ctx; narrow v1 to search + brief.

**Also measured:** live WebView2 settings read-back; SQLCipher + FTS5 encryption tests; pinned ONNX Runtime loading; UIA/WGC/OCR timings (`docs/app-compatibility.md`).

**Main risks:** 4B quality too low; WebView2 transparency/animation; WGC yellow border (Stage 0: borderless capture granted, no border); WebView2 background network traffic (**observed**, ADR-0002); default crate features pulling in network code (handled, ADR-0001); Windows' own `onnxruntime.dll` (handled, ADR-0001).

## Stage 1 — Foundation, capture, memory, baseline guardrails

**Exit criteria:**
- [ ] 8-hour run: no crash, background CPU ≤ 3 %, app RAM ≤ 250 MB.
- [ ] No image files ever written (Process Monitor + FS-watcher test on data dir and `%TEMP%`).
- [x] Blocklisted apps and private windows produce zero items. *(engine tests with simulated windows + real encrypted store; private-state detection verified on real Chrome/Edge normal and private windows; unknown → blocked)*
- [x] Title rules don't over-block: ordinary non-browser windows titled with "login", "sign in" or "password" are still captured. *(policy-level: `tsr-guard` test `generic_words_do_not_block_non_browser_windows`; end-to-end with fixture windows once capture exists)*
- [x] A UIA read never blocks the caller beyond 300 ms wall-clock, including against Word (Stage 0 measured 2.8 s without a deadline); late results are discarded. *(`tsr-capture` deadline worker; real run over 16 windows/9 apps: max 294 ms; Word/PowerPoint/Explorer cut at 285–294 ms while their abandoned reads ran to 290–732 ms)*
- [x] SQLCipher's own logging is off (`cipher_log_level = NONE`): a wrong-key open writes nothing to stderr. *(`tsr-store` test `wrong_key_writes_nothing_to_stderr`, child-process stderr capture)*
- [ ] Password text from a test WinForms/WPF app never appears in items.
- [x] Redaction corpus: 100 % of seeded secrets redacted; false positives ≤ 2 % on the clean corpus. *(290/290; 1/235 = 0.43 %; `tsr-guard` test `redaction_corpus`; thresholds in ADR-0005)*
- [x] DB unreadable without the key; no canary strings in a hex scan. *(`tsr-store` tests: wrong/zero key rejected, no plaintext header or canary in DB/WAL; key only via DPAPI `key.bin`)*

**Main risks:** UIA coverage gaps; vendored OpenSSL build friction; private-browsing detection misses.

## Stage 2 — Understanding pipeline

**Exit criteria:**
- [ ] Extraction F1 ≥ 0.70 on the held-out set; hallucinated facts ≤ 3 %; 0 schema-invalid rows stored. *(unverified: needs a model. Pipeline side measured with the oracle: 0 schema-invalid rows, 0 hallucinated stored facts, no gold fact dropped by the validators; runner ready: `tsr-eval extraction --model DIR`)*
- [ ] Injection corpus (≥ 50 cases): no injected instruction stored as a user task (unless genuinely addressed to the user); nothing triggers an action. *(code-level part met: 69 adversarial + 5 control cases pass with a SIMULATED model that obeys every injection; held-out first run was 6/12 before the guard was generalised; real-model run unverified)*
- [ ] A 2-hour synthetic workload is fully processed within 30 min after it ends. *(SIMULATED: real scheduler with assumed job costs drains 25 s after the window; real model throughput unmeasured)*
- [ ] GPU idle when nothing is queued; model unloads after the idle timeout on battery. *(tested with the SIMULATED sidecar: zero model requests when idle, unload after the battery timeout, lazy restart, unload/request race; GPU utilisation with a real model unmeasured)*
- [x] Sidecar runs with a cleared environment: setting `LLAMA_ARG_AGENT=1` or `LLAMA_ARG_HF_REPO` in the parent has no effect (tested). *(REAL llama-server b11900 in router mode: inherited `LLAMA_ARG_LOG_FILE` writes a log in a control run and nothing when Tessara starts it; fake sidecar reports only allow-listed variable names)*
- [ ] Embedders only ever use the pinned ONNX Runtime (`load_pinned`); a test confirms `System32\onnxruntime.dll` is never mapped. *(the embedder can only be built from a `PinnedRuntime`'s environment; the existing pinned-runtime test checks the mapped module; no embedding model to run end to end)*

**Main risks:** prompt injection producing misleading memory; llama.cpp flag/API changes; VRAM contention.

## Stage 3 — Notch UI

**Exit criteria:**
- [ ] Notch absent from Teams, Zoom, OBS, Snipping Tool and Xbox Game Bar captures. *(PARTIAL: absent from WGC and GDI monitor captures; the apps themselves not run)*
- [x] Expand/collapse at 60 fps; input-to-visual ≤ 100 ms. *(p95 frame 16.8 ms; input-to-visual p95 64 ms)*
- [x] No focus stealing during peeks.
- [ ] Correct placement after monitor plug/unplug and DPI change. *(PARTIAL: unit-tested; needs a live change)*

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
- **10 — Release:** signed NSIS installer and sidecar, opt-in signed updater (behind the `updater` feature, the one network exception outside `tsr-net`), SBOM per release. Gate: `cargo deny --features tessara/updater check` passes — currently blocked by `zip` advisories in `tauri-plugin-updater` 2.14.0 (ADR-0003).

See plan §17 for details.
