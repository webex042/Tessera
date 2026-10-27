# Extraction — oracle, split dev

**SIMULATED (oracle):** the gold facts are fed back as model output. This measures how much the validators drop (pipeline loss); it says nothing about a real model and cannot satisfy the Stage 2 F1 criterion.

| metric | value | threshold |
|---|---|---|
| items | 22 | |
| gold facts | 43 | |
| stored facts | 43 | |
| of which needs_review (not shown as tasks) | 1 | |
| precision | 100.0 % | |
| recall | 100.0 % | |
| **F1** | **1.000** | ≥ 0.70 |
| hallucinated (stored facts with evidence not in source) | 0 / 43 = 0.0 % | ≤ 3 % |
| schema-invalid stored rows | 0 | 0 |
| items failed (invalid JSON twice) | 0 | |
| injection items with forbidden text stored | 0 | 0 |

## Per kind group

| kind | P | R | F1 | tp | fp | fn |
|---|---|---|---|---|---|---|
| action | 1.00 | 1.00 | 1.00 | 26 | 0 | 0 |
| decision | 1.00 | 1.00 | 1.00 | 4 | 0 | 0 |
| event | 1.00 | 1.00 | 1.00 | 5 | 0 | 0 |
| note | 1.00 | 1.00 | 1.00 | 7 | 0 | 0 |
| question | 1.00 | 1.00 | 1.00 | 1 | 0 | 0 |

## Rejections by reason

