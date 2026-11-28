# Ask — qwen3-vl-4b-instruct-q4_k_m on vulkan on Vulkan0 (AMD Radeon (TM) Graphics), split all

Real model run.

Retrieval: keyword + facts (no embedder), filters from the question; facts from SIMULATED oracle extraction.

| metric | value | threshold |
|---|---|---|
| questions / invalid answers | 5 / 0 | |
| answered correctly | 1 (20.0 %) | ≥ 80 % |
| shown sentences verified | 2 / 3 (66.7 %) | ≥ 90 % |
| fabricated citations | 0 | 0 |
| first sentence, p50 | 26658 ms | ≤ 2 500 ms |
| full answer, p50 | 24952 ms | ≤ 10 000 ms |

**FAIL**
