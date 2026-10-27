# ADR-0005: Redaction v1 thresholds and context rules

- Status: Accepted (2026-10-26)
- Revisits: plan §6.3

## Context

Redaction v1 was built and measured against generated corpora (`tools/gen_redaction_corpus.py`):
290 seeded secrets in 11 categories, and 235 clean documents (155 tricky non-secret lines plus the
80 synthetic screen texts). Three plan details did not survive contact with the data.

## Decisions

1. **Generic secrets: entropy ≥ 3.0 bits/char *and* both letters and digits** (plan said ≥ 4.0).
   Hex keys cannot exceed 4.0 bits/char and real 32–40-char hex tokens score ~3.6–3.9, so 4.0
   would miss them. The keyword requirement (`key`, `token`, `secret`, `bearer`, …) stays. This
   *widens* detection.
2. **Aadhaar: a bare run of 12 digits needs an Aadhaar keyword** (`aadhaar`, `aadhar`, `UID`,
   `UIDAI`, `e-KYC`, `आधार`) within 40 bytes before it; the 4-4-4 grouped form is always redacted.
   One random 12-digit number in ten passes the Verhoeff check, and order/tracking ids are often
   12 digits. This *narrows* detection: an ungrouped Aadhaar number with no keyword nearby is not
   redacted. Mitigations: Aadhaar numbers are almost always displayed grouped; the plan's PII
   categories (phone, e-mail) are also configurable; revisit with beta data (Stage 9).
3. **Cards: no keyword required** — Luhn plus a card-network prefix and length is enough. The
   cost is the occasional order number that happens to look like a card (1 of 235 clean documents
   in the corpus). Leaking a card number is worse than blanking an order number.

## Measured (2026-10-26)

| Metric | Target (stages.md) | Result |
|---|---|---|
| Seeded secrets redacted | 100 % | 290 / 290 |
| Clean documents with any redaction | ≤ 2 % | 1 / 235 = 0.43 % (a card-like order number) |

Stage 7 grows the corpora (≥ 300 secrets incl. more Indian identifiers) and adds fuzzing.
