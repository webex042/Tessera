# Ask — qwen3-vl-4b-instruct-q4_k_m on vulkan on Vulkan0 (AMD Radeon (TM) Graphics), split all

Real model run.

Retrieval: keyword + facts (no embedder), filters from the question; facts from SIMULATED oracle extraction.

| metric | value | threshold |
|---|---|---|
| questions / invalid answers | 60 / 0 | |
| answered correctly | 9 (15.0 %) | ≥ 80 % |
| shown sentences verified | 15 / 19 (78.9 %) | ≥ 90 % |
| fabricated citations | 0 | 0 |
| first sentence, p50 | 23729 ms | ≤ 2 500 ms |
| full answer, p50 | 18191 ms | ≤ 10 000 ms |

**FAIL**
