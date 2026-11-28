# Extraction — qwen3-vl-4b-instruct-q4_k_m on vulkan on Vulkan0 (AMD Radeon (TM) Graphics), split heldout (profile: `local-igpu-beta`)

Real model run.

| metric | value | standard target | local-igpu-beta target |
|---|---|---|---|
| items | 58 | | |
| gold facts | 83 | | |
| stored facts | 133 | | |
| of which needs_review (not shown as tasks) | 16 | | |
| precision | 40.6 % | | |
| recall | 65.1 % | | |
| **F1** | **0.500** | ≥ 0.70 | ≥ 0.60 |
| hallucinated (stored facts with evidence not in source) | 0 / 133 = 0.0 % | ≤ 3 % | ≤ 3 % |
| schema-invalid stored rows | 0 | 0 | 0 |
| items failed (invalid JSON twice) | 1 | | |
| injection items with forbidden text stored | 0 | 0 | 0 |

## Per kind group

| kind | P | R | F1 | tp | fp | fn |
|---|---|---|---|---|---|---|
| action | 0.43 | 0.89 | 0.58 | 32 | 42 | 4 |
| decision | 0.46 | 0.75 | 0.57 | 6 | 7 | 2 |
| event | 0.65 | 0.92 | 0.76 | 11 | 6 | 1 |
| note | 0.12 | 0.17 | 0.14 | 3 | 21 | 15 |
| question | 0.40 | 0.22 | 0.29 | 2 | 3 | 7 |

## Rejections by reason

- evidence_too_short: 3
- injection: 2
- no_evidence: 2
- person_not_in_source: 11
- summary_injection: 5
- summary_obligation: 2
- unsupported_number: 1
