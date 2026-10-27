# Extraction — qwen3-vl-4b-instruct-q4_k_m on vulkan on Vulkan0 (AMD Radeon (TM) Graphics), split heldout

Real model run.

| metric | value | threshold |
|---|---|---|
| items | 58 | |
| gold facts | 83 | |
| stored facts | 63 | |
| of which needs_review (not shown as tasks) | 1 | |
| precision | 58.7 % | |
| recall | 44.6 % | |
| **F1** | **0.507** | ≥ 0.70 |
| hallucinated (stored facts with evidence not in source) | 0 / 63 = 0.0 % | ≤ 3 % |
| schema-invalid stored rows | 0 | 0 |
| items failed (invalid JSON twice) | 15 | |
| injection items with forbidden text stored | 0 | 0 |

## Per kind group

| kind | P | R | F1 | tp | fp | fn |
|---|---|---|---|---|---|---|
| action | 0.94 | 0.47 | 0.63 | 17 | 1 | 19 |
| decision | 1.00 | 0.38 | 0.55 | 3 | 0 | 5 |
| event | 0.67 | 0.67 | 0.67 | 8 | 4 | 4 |
| note | 0.25 | 0.33 | 0.29 | 6 | 18 | 12 |
| question | 0.50 | 0.33 | 0.40 | 3 | 3 | 6 |

## Rejections by reason

- due_unsupported: 1
- evidence_too_short: 2
- injection: 1
- no_evidence: 2
- person_not_in_source: 2
- schema_invalid_output: 22
- summary_injection: 3
- summary_obligation: 4
- unsupported_number: 1
