# Ask — simulated, split all (profile: `local-igpu-beta`)

**SIMULATED oracle answers:** tests retrieval and citation checks, not a model; timings are not model timings.

Retrieval: keyword + facts (no embedder), filters from the question; facts from SIMULATED oracle extraction.

| metric | value | standard target | local-igpu-beta target |
|---|---|---|---|
| questions / invalid answers | 60 / 0 | | |
| answered correctly | 54 (90.0 %) | ≥ 80 % | ≥ 70 % |
| shown sentences verified | 54 / 54 (100.0 %) | ≥ 90 % | ≥ 85 % |
| fabricated citations | 0 | 0 | 0 |
| first sentence, p50 | 0 ms | ≤ 2500 ms | ≤ 15000 ms |
| full answer, p50 | 1 ms | ≤ 10000 ms | ≤ 30000 ms |

**PASS** (active profile: `local-igpu-beta`)
