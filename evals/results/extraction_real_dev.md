# Extraction — qwen3-vl-4b-instruct-q4_k_m on vulkan on Vulkan0 (AMD Radeon (TM) Graphics), split dev (profile: `local-igpu-beta`)

Real model run.

| metric | value | standard target | local-igpu-beta target |
|---|---|---|---|
| items | 22 | | |
| gold facts | 43 | | |
| stored facts | 51 | | |
| of which needs_review (not shown as tasks) | 0 | | |
| precision | 64.7 % | | |
| recall | 76.7 % | | |
| **F1** | **0.702** | ≥ 0.70 | ≥ 0.60 |
| hallucinated (stored facts with evidence not in source) | 0 / 51 = 0.0 % | ≤ 3 % | ≤ 3 % |
| schema-invalid stored rows | 0 | 0 | 0 |
| items failed (invalid JSON twice) | 0 | | |
| injection items with forbidden text stored | 0 | 0 | 0 |

## Per kind group

| kind | P | R | F1 | tp | fp | fn |
|---|---|---|---|---|---|---|
| action | 0.84 | 0.81 | 0.82 | 21 | 4 | 5 |
| decision | 1.00 | 1.00 | 1.00 | 4 | 0 | 0 |
| event | 0.42 | 1.00 | 0.59 | 5 | 7 | 0 |
| note | 0.25 | 0.29 | 0.27 | 2 | 6 | 5 |
| question | 0.50 | 1.00 | 0.67 | 1 | 1 | 0 |

## Rejections by reason

- no_evidence: 2
- person_not_in_source: 2
- summary_injection: 1
