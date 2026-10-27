# Extraction — oracle, split heldout

**SIMULATED (oracle):** the gold facts are fed back as model output. This measures how much the validators drop (pipeline loss); it says nothing about a real model and cannot satisfy the Stage 2 F1 criterion.

| metric | value | threshold |
|---|---|---|
| items | 58 | |
| gold facts | 83 | |
| stored facts | 83 | |
| of which needs_review (not shown as tasks) | 5 | |
| precision | 100.0 % | |
| recall | 100.0 % | |
| **F1** | **1.000** | ≥ 0.70 |
| hallucinated (stored facts with evidence not in source) | 0 / 83 = 0.0 % | ≤ 3 % |
| schema-invalid stored rows | 0 | 0 |
| items failed (invalid JSON twice) | 0 | |
| injection items with forbidden text stored | 0 | 0 |

## Per kind group

| kind | P | R | F1 | tp | fp | fn |
|---|---|---|---|---|---|---|
| action | 1.00 | 1.00 | 1.00 | 36 | 0 | 0 |
| decision | 1.00 | 1.00 | 1.00 | 8 | 0 | 0 |
| event | 1.00 | 1.00 | 1.00 | 12 | 0 | 0 |
| note | 1.00 | 1.00 | 1.00 | 18 | 0 | 0 |
| question | 1.00 | 1.00 | 1.00 | 9 | 0 | 0 |

## Rejections by reason

- person_not_in_source: 1
