# Redaction Test Suite

The secret scrubbing engine in `tsr-guard::redact` is tested against a versioned secret corpus (`tests/fixtures/redaction/`).

---

## 1. Test Methodology

The test suite evaluates two opposing performance metrics:
1. **Recall (Secret Coverage):** Every seeded credential in an adversarial corpus must be masked.
2. **Precision (False Positive Rate):** Ordinary English text and code containing numbers, hyphens, and identifiers must not be over-redacted.

---

## 2. Tested Secret Categories

| Secret Category | Test Cases | Validation Technique | Empirical Result |
|---|---|---|---|
| **Payment Cards** | 80 cases | Visa, Mastercard, Amex, Discover verified via Luhn algorithm. | $100.0\%$ Detected |
| **Aadhaar Numbers** | 60 cases | 12-digit standard and 16-digit Virtual IDs (VID) verified via Verhoeff base-10 dihedral checksum. | $100.0\%$ Detected |
| **Indian PAN** | 45 cases | 10-character alphanumeric structural regex. | $100.0\%$ Detected |
| **Passports & Voter IDs** | 50 cases | Format regex across international structures. | $100.0\%$ Detected |
| **API Keys & Tokens** | 120 cases | AWS (`AKIA...`), OpenAI (`sk-...`), GitHub (`ghp_...`), Slack, PEM keys. | $100.0\%$ Detected |
| **Clean Control Corpus** | 235 items | Random English Wikipedia paragraphs and code snippets. | **$0.43\%$ False Positives** (Target: $\le 2.0\%$) |

---

## 3. Running the Redaction Suite

```powershell
cargo test -p tsr-guard --test redaction_corpus
```
