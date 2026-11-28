---
title: Redaction Test Suite
description: Deterministic secret redaction recall across 355 seeded secrets and 242 clean control items.
---

# Redaction Test Suite

The secret scrubbing engine in `tsr-guard::redact` is tested against a versioned synthetic corpus (`tests/fixtures/redaction/` plus clean extraction fixtures) via `crates/tsr-guard/tests/redaction_corpus.rs`.

---

## 1. Test Methodology

The test suite enforces two opposing invariants:
1. **Recall (Secret Coverage):** `100%` of seeded secrets across all categories must be masked (including when hyphens or spaces are stripped).
2. **Precision (False-Positive Ceiling):** False positives across `tests/fixtures/redaction/clean.txt` paired with all `80` `evals/extraction/items/*.json` texts (`242` total clean documents) must remain $\le 2.0\%$.

---

## 2. Verified Corpus Breakdown (`cargo test -p tsr-guard --test redaction_corpus --locked`)

| Secret Category | Seeded Cases | Empirical Result |
|---|---|---|
| `aadhaar` | `50` | **50 / 50 (100.0%)** |
| `account` | `35` | **35 / 35 (100.0%)** |
| `api_key` | `54` | **54 / 54 (100.0%)** |
| `card` | `40` | **40 / 40 (100.0%)** |
| `jwt` | `10` | **10 / 10 (100.0%)** |
| `otp` | `40` | **40 / 40 (100.0%)** |
| `pan` | `20` | **20 / 20 (100.0%)** |
| `passport` | `15` | **15 / 15 (100.0%)** |
| `password` | `26` | **26 / 26 (100.0%)** |
| `private_key` | `6` | **6 / 6 (100.0%)** |
| `secret` | `24` | **24 / 24 (100.0%)** |
| `upi` | `20` | **20 / 20 (100.0%)** |
| `voter_id` | `15` | **15 / 15 (100.0%)** |
| **Total Seeded Secrets** | **355** | **355 / 355 (100.0%)** <StatusBadge status="pass" /> |
| **Clean Control Corpus** | **242** | **1 / 242 = 0.41% FP** (Target: $\le 2.0\%$) <StatusBadge status="pass" /> |

*(The single benign false positive is `clean:28`: `Order #[REDACTED:card] shipped on 20 Jul`.)*

---

## 3. Running the Redaction Suite

```powershell
cargo test -p tsr-guard --test redaction_corpus --locked -- --nocapture
```
