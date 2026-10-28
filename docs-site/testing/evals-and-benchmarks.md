# Evaluations & Benchmarks

The `tsr-eval` runner (`evals/runner`) evaluates real pipeline performance against synthetic, versioned datasets (`evals/`).

---

## Evaluation Datasets

| Dataset | File Location | Size | Contents |
|---|---|---|---|
| **Extraction** | `evals/extraction/items/*.json` | 80 items (22 dev, 58 held-out) | Synthetic desktop screen texts paired with human-verified gold facts. |
| **Ask Queries** | `evals/ask/ask.json` | 60 questions (8 unanswerable) | Complex past-oriented questions paired with gold item IDs and citation requirements. |
| **Draft Scenarios** | `evals/draft/draft.json` | 20 scenarios | Thread contexts requiring context-aware ghost reply drafts. |
| **Injection** | `tests/fixtures/injection/cases.json` | 74 cases | Adversarial prompt injections and control items. |

---

## Detailed Benchmark Results (AMD Radeon iGPU)

Source artifact: `evals/results/extraction_real_heldout.md`

### Extraction Breakdown (Held-Out Set)
- **Total Test Items:** 58 screen sessions.
- **Gold Target Facts:** 83 facts.
- **Extracted Stored Facts:** 63 facts.
- **Precision:** $58.7\%$
- **Recall:** $44.6\%$
- **$F_1$ Score:** **0.507** (Target: $\ge 0.70$ <StatusBadge status="fail" />)
- **Hallucinated Facts:** **0 / 63 (0.0%)** (Target: $\le 3.0\%$ <StatusBadge status="pass" />)
- **Schema-Invalid Stored Rows:** **0 rows** (Target: 0 <StatusBadge status="pass" />)

### Per-Kind Group Performance
| Kind | Precision | Recall | F1 Score | True Positives | False Positives | False Negatives |
|---|---|---|---|---|---|---|
| **Action** | 0.94 | 0.47 | **0.63** | 17 | 1 | 19 |
| **Decision** | 1.00 | 0.38 | **0.55** | 3 | 0 | 5 |
| **Event** | 0.67 | 0.67 | **0.67** | 8 | 4 | 4 |
| **Note** | 0.25 | 0.33 | **0.29** | 6 | 18 | 12 |
| **Question** | 0.50 | 0.33 | **0.40** | 3 | 3 | 6 |

### Rejection Reasons (Why Facts Were Dropped)
- `schema_invalid_output`: 22
- `summary_obligation`: 4
- `summary_injection`: 3
- `no_evidence`: 2
- `evidence_too_short`: 2
- `person_not_in_source`: 2
- `injection`: 1
- `unsupported_number`: 1
