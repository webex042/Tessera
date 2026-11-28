---
title: Evaluations & Benchmarks
description: Real-model and simulated evaluation datasets, hardware-calibrated methodology, and verified 2026-11-28 results.
---

# Evaluations & Benchmarks

The `tsr-eval` runner (`evals/runner`) scores Tessera's actual Rust pipelines (`tsr-pipeline`, `tsr-retrieval`, `tsr-draft`, `tsr-guard`, `tsr-infer`) against versioned synthetic datasets in `evals/` and `tests/fixtures/`.

For full profile rules and failure analysis, see [Measured Status and Limitations](/guide/limitations).

---

## Evaluation Datasets

| Dataset | File Location | Size | Split / Purpose |
|---|---|---|---|
| **Extraction** | `evals/extraction/items/*.json` | `80` items (`22` `dev`, `58` `heldout`), `126` gold facts (`83` held-out) | Synthetic desktop sessions paired with gold facts & verbatim evidence |
| **Ask Queries** | `evals/ask/ask.json` | `60` questions (`12` `dev`, `48` `heldout`; `8` unanswerable) | Past-oriented queries with gold item IDs and citation rules |
| **Draft Scenarios** | `evals/draft/draft.json` | `20` scenarios | Thread contexts for grounded reply generation & fact-checking |
| **Primary Injection** | `tests/fixtures/injection/cases.json` | `152` cases (`135` adversarial, `17` genuine controls) | Prompt-injection corpus across 38 categories (`tools/gen_injection_corpus.py`) |
| **Unseen 3rd Injection** | `tests/fixtures/injection/unseen_v3.json` | `20` cases (`15` adversarial, `5` genuine controls) | Independent unseen phishing, Hinglish, comment, and VLM cases |

---

## Latest Benchmark Results (`2026-11-28`, AMD Radeon iGPU)

Hardware: **AMD Ryzen 5 7530U (AMD Radeon Integrated Graphics via Vulkan)**, Windows 11, running **`Qwen3-VL-4B-Instruct-Q4_K_M`** (`standard` target = RTX 4050, unavailable).

### 1. Blind Held-Out Extraction (`evals/results/extraction_real_heldout.md`)

- **Test Set:** `58` blind held-out items (`83` gold facts).
- **Stored Facts:** `133` total (`16` held as `needs_review`).
- **Precision / Recall / F1:** **40.6%** P / **65.1%** R / **0.500 F1** (<StatusBadge status="fail" /> vs. `>= 0.60` `local-igpu-beta` and `>= 0.70` `standard`).
  - *Split Comparison:* On the 22-item `dev` split (`extraction_real_dev.md`), F1 reached **0.702** (`64.7%` P, `76.7%` R). Under `--model oracle` (`extraction_oracle_all.md`), F1 is **0.996** (`125/126`).
- **Hallucinated Stored Facts:** **0 / 133 (0.0%)** (<StatusBadge status="pass" />, target $\le 3.0\%$).
- **Schema-Invalid Stored Rows:** **0 rows** (`1` item failed valid JSON twice; `0` injection items stored forbidden text) (<StatusBadge status="pass" />).

| Kind Group | Precision | Recall | F1 | TP | FP | FN |
|---|---|---|---|---|---|---|
| **Action** | `0.43` | `0.89` | **0.58** | `32` | `42` | `4` |
| **Decision** | `0.46` | `0.75` | **0.57** | `6` | `7` | `2` |
| **Event** | `0.65` | `0.92` | **0.76** | `11` | `6` | `1` |
| **Note** | `0.12` | `0.17` | **0.14** | `3` | `21` | `15` |
| **Question** | `0.40` | `0.22` | **0.29** | `2` | `3` | `7` |

Rejection breakdown: `person_not_in_source` (`11`), `summary_injection` (`5`), `evidence_too_short` (`3`), `injection` (`2`), `no_evidence` (`2`), `summary_obligation` (`2`), `unsupported_number` (`1`).

---

### 2. Blind Held-Out Ask (`ask_real_heldout_r{1,2,3}.md`)

Measured across **3 independent blind held-out runs** (`48` unseen questions per run):

- **Accuracy:** **77.1% ± 0.0%** (`37 / 48` on R1, R2, and R3) — <StatusBadge status="pass" /> (`local-igpu-beta >= 70%`; `dev` split = `91.7%` on `12` q).
- **Verified Sentences:** **92.3% ± 0.0%** (`36 / 39`) — <StatusBadge status="pass" /> (`>= 85%` beta / `>= 90%` standard).
- **Fabricated Citations:** **0** across all runs — <StatusBadge status="pass" />.
- **First Sentence Latency (p50):** **14,240 ± 516 ms** (`13,787 ms`, `14,802 ms`, `14,132 ms`) — <StatusBadge status="pass" /> (`<= 15,000 ms` beta; <StatusBadge status="fail" /> vs. `<= 2,500 ms` standard).
- **Full Answer Latency (p50):** **15,315 ± 490 ms** (`15,127 ms`, `15,871 ms`, `14,947 ms`) — <StatusBadge status="pass" /> (`<= 30,000 ms` beta; <StatusBadge status="fail" /> vs. `<= 10,000 ms` standard).

---

### 3. Draft Assistant & Hybrid ONNX Search (`draft_real_all.md`, `search_e5.md`)

- **Draft (`20` scenarios, `local-igpu-beta`):** **20 / 20 valid**, **0** unflagged unsupported claims, `16 / 20` must-include, `20 / 20` must-not-include, `19 / 20` placeholders when expected, **5,587 ms** first visible text p50, **14,175 ms** complete p50 (<StatusBadge status="pass" /> beta; <StatusBadge status="fail" /> standard latency).
- **Search (`multilingual-e5-small` ONNX, 54 questions / 80 items):**
  - Keyword (`FTS5`): `R@5 0.936`, `R@10 0.962`, `MRR 0.841`, `nDCG@10 0.866`.
  - Vector (`multilingual-e5-small`): `R@5 0.942`, `R@10 0.962`, `MRR 0.806`, `nDCG@10 0.842`.
  - Keyword + Vector (RRF): `R@5 0.962`, `R@10 0.981`, `MRR 0.907`, `nDCG@10 0.921`.
  - **Keyword + Vector + Facts (RRF):** **R@5 0.981**, **R@10 1.000**, **P@5 0.219**, **MRR 0.900**, **nDCG@10 0.921**, plus **6 / 6 behaviour checks PASS** (<StatusBadge status="pass" />).
