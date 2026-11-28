# Search evaluation

54 answerable questions (Ask set), 80 indexed items. **Vector numbers use the SIMULATED hashing embedder** (spelling overlap, not meaning); real E5 numbers are not measured on this machine.

| channels | recall@5 | recall@10 | precision@5 | MRR | nDCG@10 |
|---|---|---|---|---|---|
| keyword | 0.942 | 0.962 | 0.212 | 0.828 | 0.856 |
| keyword + vector (RRF) | 0.913 | 0.981 | 0.204 | 0.838 | 0.864 |
| keyword + vector + facts (RRF) | 0.942 | 1.000 | 0.212 | 0.845 | 0.877 |
| vector (SIMULATED embedder) | 0.750 | 0.846 | 0.169 | 0.649 | 0.687 |

Unanswerable questions with at least one hit (hybrid+facts): 8 of 8 — retrieval always returns something; answering "not in memory" is the Ask step's job (Stage 6).

No claim is made that hybrid beats keyword in general: the table above is the only evidence, on this set, with a simulated embedder.

## Behaviour checks

- PASS suppressed person: 0 items with the forgotten name returned (filtered 7; 6 first-name-only mentions returned, known limitation)
- PASS blocklist rule added later: no Slack item returned (filtered 8)
- PASS redacted OTP value: no keyword match, no returned text contains it
- PASS duplicate document: both copies ranked (2/2), each with its own item id
- PASS no vector index: keyword results returned and diagnostics say vector unavailable
- PASS same query, same ranking
