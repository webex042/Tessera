# Search evaluation

54 answerable questions (Ask set), 80 indexed items. Real `multilingual-e5-small` ONNX embeddings on pinned ONNX Runtime.

| channels | recall@5 | recall@10 | precision@5 | MRR | nDCG@10 |
|---|---|---|---|---|---|
| keyword | 0.936 | 0.962 | 0.208 | 0.841 | 0.866 |
| keyword + vector (RRF) | 0.962 | 0.981 | 0.215 | 0.907 | 0.921 |
| keyword + vector + facts (RRF) | 0.981 | 1.000 | 0.219 | 0.900 | 0.921 |
| vector (multilingual-e5-small) | 0.942 | 0.962 | 0.212 | 0.806 | 0.842 |

Unanswerable questions with at least one hit (hybrid+facts): 8 of 8 — retrieval always returns something; answering "not in memory" is the Ask step's job (Stage 6).


## Behaviour checks

- PASS suppressed person: 0 items with the forgotten name returned (filtered 7; 5 first-name-only mentions returned, known limitation)
- PASS blocklist rule added later: no Slack item returned (filtered 9)
- PASS redacted OTP value: no keyword match, no returned text contains it
- PASS duplicate document: both copies ranked (2/2), each with its own item id
- PASS no vector index: keyword results returned and diagnostics say vector unavailable
- PASS same query, same ranking
