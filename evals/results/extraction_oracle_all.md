# Extraction — oracle, split all (profile: `standard`)

**SIMULATED (oracle):** the gold facts are fed back as model output. This measures how much the validators drop (pipeline loss); it says nothing about a real model and cannot satisfy the Stage 2 F1 criterion.

| metric | value | standard target | local-igpu-beta target |
|---|---|---|---|
| items | 80 | | |
| gold facts | 126 | | |
| stored facts | 125 | | |
| of which needs_review (not shown as tasks) | 6 | | |
| precision | 100.0 % | | |
| recall | 99.2 % | | |
| **F1** | **0.996** | ≥ 0.70 | ≥ 0.60 |
| hallucinated (stored facts with evidence not in source) | 0 / 125 = 0.0 % | ≤ 3 % | ≤ 3 % |
| schema-invalid stored rows | 0 | 0 | 0 |
| items failed (invalid JSON twice) | 0 | | |
| injection items with forbidden text stored | 0 | 0 | 0 |

## Per kind group

| kind | P | R | F1 | tp | fp | fn |
|---|---|---|---|---|---|---|
| action | 1.00 | 1.00 | 1.00 | 62 | 0 | 0 |
| decision | 1.00 | 1.00 | 1.00 | 12 | 0 | 0 |
| event | 1.00 | 1.00 | 1.00 | 17 | 0 | 0 |
| note | 1.00 | 0.96 | 0.98 | 24 | 0 | 1 |
| question | 1.00 | 1.00 | 1.00 | 10 | 0 | 0 |

## Rejections by reason

- person_not_in_source: 1
- redundant_note: 1
