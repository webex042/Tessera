# Stage 6 verification report: Ask with sources

Date: 2026-10-27. Decisions: [ADR-0008](../adr/0008-stages-4-6-and-real-model-guard.md).

## A. What was built

| task | where |
|---|---|
| Query filter parser (past-oriented time, app, fact kind) | `tsr-pipeline::askq` |
| Retrieval: keyword + facts (RRF), filters, top 8 snippets ≤ ~1 500 tokens, one per source | `tsr-retrieval::{hybrid, ask::snippets}`; `src-tauri/src/ask.rs` |
| Ask prompt, streaming sentences (incremental JSON), per-sentence citation verification | `tsr-retrieval::ask::{request, SentenceStream, verify, answer}` |
| "Insufficient" / nothing verified → "I couldn't find that" + top 3 sources | `answer`, `AskView` |
| Ask tab, citation chips that open the item, Ctrl+Alt+K | `ui/src/notch/views/AskView.tsx` |

## B. Exit criteria

| criterion | measured | evidence | status |
|---|---|---|---|
| ≥ 80 % answered correctly (60 questions) | SIMULATED oracle answers: 53 / 60 = 88.3 % (the 7 misses are retrieval misses). REAL ask/v1: 9 / 60 = 15 % (28 % counting verified content); REAL ask/v2 (sorted keys): 9 / 60 = 15 %; **REAL ask/v2, declared key order (ADR-0010): 40 / 60 = 66.7 %** | `tsr-eval ask`; `evals/results/ask_real_all.*`, `ask_real_v2_sortedkeys.*` | pipeline **PASS** (SIMULATED); real model **FAIL** |
| ≥ 90 % of shown sentences verified | SIMULATED 53 / 53; REAL v1 78.9 %; REAL v2 sorted keys 4 / 4 (only 4 shown); **declared order 41 / 49 = 83.7 %** | same | **FAIL** |
| 0 fabricated citations | SIMULATED 0; REAL v1 0; REAL v2 0 (both key orders); unit tests: a cited number that does not exist is counted and never verifies | `ask::tests` | **PASS** |
| First token p50 ≤ 2.5 s, full ≤ 10 s | REAL (AMD iGPU) v1: first sentence 23.7 s, full 18.2 s; v2 sorted keys: 14.5 s / 19.2 s; v2 declared order: 25.2 s / 25.5 s (longer answers) | same | **FAIL** (target RTX 4050 not available) |

Real model: Qwen3-VL-4B Q4_K_M on Vulkan (integrated GPU), facts from SIMULATED oracle extraction.
With ask/v1 the model set "insufficient" on 57 / 60 questions. With ask/v2, 48 / 60 answers had no shown sentence and
no "insufficient" flag, 5 were "insufficient", 4 showed one sentence and 3 were invalid JSON
(`ask_real_all.json`), so the code shows "I couldn't find that" for nearly everything. Whether the
model wrote nothing or wrote sentences that failed verification is not yet known: that run did
not record raw output. The v1 result file was overwritten by
the v2 run; v1 numbers are from the run log. Diagnosis tooling: `tsr-eval ask --ids …` records
the raw model output per question.

**Root cause of the empty answers (ADR-0010).** Raw outputs recorded with `--ids` showed
`{"insufficient": false, "sentences": []}` for 5 / 5 questions. The schema was sent with its
keys sorted, so the grammar made the model commit to `insufficient` before writing any answer.
With `serde_json/preserve_order` the same prompt (ask/v2, unchanged) scores 40 / 60 correct,
0 invalid answers, 0 fabricated citations; 8 of 49 shown sentences failed verification and were
hidden. Of the 60: 34 answered and correct, 11 answered but wrong or incomplete, 6 correctly
"insufficient", 8 wrongly "insufficient", 1 empty. There is no held-out split for Ask: ask/v2
was written after seeing ask/v1 fail on these same 60 questions, so 66.7 % may be optimistic.

Retrieval runs without an embedder (none approved); the vector channel is measured only with
the SIMULATED hash embedder in `evals/results/search.md`.
