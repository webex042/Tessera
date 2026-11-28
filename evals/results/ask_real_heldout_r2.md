# Ask — qwen3-vl-4b-instruct-q4_k_m on vulkan on Vulkan0 (AMD Radeon (TM) Graphics), split heldout (profile: `local-igpu-beta`)

Real model run.

Retrieval: keyword + E5 vector + facts (RRF), filters from the question; facts from SIMULATED oracle extraction.

| metric | value | standard target | local-igpu-beta target |
|---|---|---|---|
| questions / invalid answers | 48 / 0 | | |
| answered correctly | 37 (77.1 %) | ≥ 80 % | ≥ 70 % |
| shown sentences verified | 36 / 39 (92.3 %) | ≥ 90 % | ≥ 85 % |
| fabricated citations | 0 | 0 | 0 |
| first sentence, p50 | 14802 ms | ≤ 2500 ms | ≤ 15000 ms |
| full answer, p50 | 15871 ms | ≤ 10000 ms | ≤ 30000 ms |

**PASS** (active profile: `local-igpu-beta`)
