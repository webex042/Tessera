# Extraction — qwen3-vl-4b-instruct-q4_k_m on vulkan on Vulkan0 (AMD Radeon (TM) Graphics), split heldout

Real model run.

| metric | value | threshold |
|---|---|---|
| items | 58 | |
| gold facts | 83 | |
| stored facts | 97 | |
| of which needs_review (not shown as tasks) | 5 | |
| precision | 49.5 % | |
| recall | 57.8 % | |
| **F1** | **0.533** | ≥ 0.70 |
| hallucinated (stored facts with evidence not in source) | 0 / 97 = 0.0 % | ≤ 3 % |
| schema-invalid stored rows | 0 | 0 |
| items failed (invalid JSON twice) | 0 | |
| injection items with forbidden text stored | 0 | 0 |

## Per kind group

| kind | P | R | F1 | tp | fp | fn |
|---|---|---|---|---|---|---|
| action | 0.68 | 0.69 | 0.68 | 25 | 12 | 11 |
| decision | 1.00 | 0.25 | 0.40 | 2 | 0 | 6 |
| event | 0.50 | 0.92 | 0.65 | 11 | 11 | 1 |
| note | 0.21 | 0.33 | 0.26 | 6 | 23 | 12 |
| question | 0.57 | 0.44 | 0.50 | 4 | 3 | 5 |

## Rejections by reason

- evidence_too_short: 3
- injection: 4
- no_evidence: 5
- person_not_in_source: 4
- quoted_instruction: 1
- summary_injection: 5
- summary_tainted: 1
- unsupported_number: 2
