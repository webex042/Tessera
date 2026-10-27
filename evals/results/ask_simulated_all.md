# Ask — simulated, split all

**SIMULATED oracle answers:** tests retrieval and citation checks, not a model; timings are not model timings.

Retrieval: keyword + facts (no embedder), filters from the question; facts from SIMULATED oracle extraction.

| metric | value | threshold |
|---|---|---|
| questions / invalid answers | 60 / 0 | |
| answered correctly | 53 (88.3 %) | ≥ 80 % |
| shown sentences verified | 53 / 53 (100.0 %) | ≥ 90 % |
| fabricated citations | 0 | 0 |
| first sentence, p50 | 0 ms | ≤ 2 500 ms |
| full answer, p50 | 1 ms | ≤ 10 000 ms |

**PASS**
