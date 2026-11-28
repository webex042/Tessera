# Extraction — qwen3-vl-4b-instruct-q4_k_m on vulkan on Vulkan0 (AMD Radeon (TM) Graphics), split heldout

Real model run.

| metric | value | threshold |
|---|---|---|
| items | 12 | |
| gold facts | 16 | |
| stored facts | 20 | |
| of which needs_review (not shown as tasks) | 1 | |
| precision | 50.0 % | |
| recall | 62.5 % | |
| **F1** | **0.556** | ≥ 0.70 |
| hallucinated (stored facts with evidence not in source) | 0 / 20 = 0.0 % | ≤ 3 % |
| schema-invalid stored rows | 0 | 0 |
| items failed (invalid JSON twice) | 0 | |
| injection items with forbidden text stored | 0 | 0 |

## Per kind group

| kind | P | R | F1 | tp | fp | fn |
|---|---|---|---|---|---|---|
| action | 0.40 | 0.40 | 0.40 | 2 | 3 | 3 |
| event | 0.62 | 1.00 | 0.77 | 5 | 3 | 0 |
| note | 0.00 | 0.00 | 0.00 | 0 | 3 | 1 |
| question | 0.75 | 0.60 | 0.67 | 3 | 1 | 2 |

## Rejections by reason

- evidence_too_short: 1
- summary_injection: 1
