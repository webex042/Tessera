# Ask — qwen3-vl-4b-instruct-q4_k_m on vulkan on Vulkan0 (AMD Radeon (TM) Graphics), split all

Real model run.

Retrieval: keyword + facts (no embedder), filters from the question; facts from SIMULATED oracle extraction.

| metric | value | threshold |
|---|---|---|
| questions / invalid answers | 60 / 0 | |
| answered correctly | 40 (66.7 %) | ≥ 80 % |
| shown sentences verified | 41 / 49 (83.7 %) | ≥ 90 % |
| fabricated citations | 0 | 0 |
| first sentence, p50 | 25155 ms | ≤ 2 500 ms |
| full answer, p50 | 25540 ms | ≤ 10 000 ms |

**FAIL**
