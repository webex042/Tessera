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
| ≥ 80 % answered correctly (60 questions) | SIMULATED oracle answers: 53 / 60 = 88.3 % (the 7 misses are retrieval misses); real model: running | `tsr-eval ask`; `evals/results/ask_*` | pipeline **PASS** (SIMULATED); real model see results |
| ≥ 90 % of shown sentences verified | SIMULATED: 53 / 53; real model: running | same | see results |
| 0 fabricated citations | SIMULATED: 0; unit tests: a cited number that does not exist is counted and never verifies | `ask::tests` | see results |
| First token p50 ≤ 2.5 s, full ≤ 10 s | real model: running | same | see results |

Retrieval runs without an embedder (none approved); the vector channel is measured only with
the SIMULATED hash embedder in `evals/results/search.md`.
