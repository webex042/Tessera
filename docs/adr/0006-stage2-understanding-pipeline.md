# ADR-0006: Stage 2 understanding-pipeline decisions

- Status: Accepted (2026-10-27)
- Revisits: plan §7.1, §7.3, §8.3–8.5, §9.2, §10.3, §16.2; stages.md Stage 2

## Context

Stage 2 built the inference runtime, model setup, extraction, the resolvers, the facts store,
retrieval, rung 3 and the eval runners. Building and testing them changed or sharpened several
plan details. Each item says whether it **widens** or **narrows** a guarantee.

## Decisions

### Inference runtime

1. **Sidecar starts suspended inside the Job Object** (plan: "assigned to a Job Object"). The
   process is created with `CREATE_SUSPENDED`, assigned, then resumed, so it never runs outside
   the job. *Widens.*
2. **The sidecar endpoint is authenticated by port owner.** Before the first `/health` is
   trusted, `GetExtendedTcpTable` must show that the listening socket on `127.0.0.1:<port>`
   belongs to the child we started. This closes the bind-then-spawn race where another local
   program could take the port and pose as the model server. *Widens.*
3. **No `hyper`/`ureq` in `tsr-infer`.** The loopback client is a ~350-line HTTP/1.1 client on
   `std::net` (one request per connection, deadlines, size limits, cancellation). Plan allowed
   "hyper without TLS or ureq with TLS disabled"; this keeps every HTTP library out of the
   non-network crates. *Neutral.*
4. **Model file names.** The published files are `Qwen3VL-4B-Instruct-*.gguf` and
   `mmproj-Qwen3VL-4B-Instruct-F16.gguf` (no dash), not the plan's `Qwen3-VL-...`. Digests and
   sizes are pinned in `tools/models.lock` at full commit revisions, read from the Hugging Face
   API; small config files carry a git blob SHA-1 (the only digest the API publishes for
   non-LFS files). *Neutral.*
5. **Downloads follow redirects by hand** and only to `huggingface.co`, `*.huggingface.co`,
   `hf.co`, `*.hf.co`; proxies are ignored; TLS is the OS stack (SChannel). *Narrows network.*

### Extraction and facts

6. **Name aliases are not unique across people** (plan schema: `UNIQUE(kind, alias)`). Two
   different people can share a name; only e-mail addresses and handles are unique. A name links
   to an existing person only when exactly one person has it and no identifier contradicts it.
   *Widens correctness.*
7. **Suppressed aliases are stored as keyed SHA-256 digests** (random per-database salt), not
   as text. Forgetting a person keeps no readable trace of their names. Consequence: suppression
   matches exact normalised aliases, so a first-name-only mention ("Kenji") of a forgotten
   "Kenji Watanabe" is not caught unless "kenji" was an alias. *Widens privacy; documented gap.*
8. **Evidence is stored as the source span**, not the model's quote: the stored quote is the
   exact item text at `start_ch..end_ch` (exact, normalised or fuzzy match). A fact is deleted by
   trigger when its last evidence row is deleted (e.g. its items were deleted). *Widens.*
9. **Fuzzy evidence uses Dice ≥ 0.9 over word sets in a sliding window**, only for quotes of ≥ 4
   words; quotes under 2 words or 8 characters are refused (plan: "token-set ratio ≥ 0.9").
   De-duplication uses the same word-set Dice ≥ 0.85 plus same kind, same owner and compatible
   due date. *Neutral (made concrete).*
10. **Numbers in a fact's text must occur in the source**, after the second redaction (so a
    secret the model copies is redacted rather than rejecting the fact). Owners named in a fact
    must occur in the source; unsupported counterparties and due texts are dropped (counted).
    *Widens.*
11. **Instruction classes** (`tsr-pipeline::guard`): injection → rejected (any kind); quoted,
    forwarded or code-block instructions → not stored as tasks; an imperative task for the user
    with no sign it addresses the user → `needs_review` (new fact status); **money movement**
    (wire/transfer/pay to an account, gift cards, crypto) → always `needs_review`. Judged on the
    evidence *sentence*; an injection elsewhere on the line taints it only if the sentence is not
    addressed to the user; an injection line taints the next line. *Widens; costs: 5 of 83
    held-out gold facts (1 of 43 dev) land in `needs_review` instead of open (oracle run).*
12. **Summaries are descriptive only.** A summary is withheld if it uses obligation language
    ("must", "needs to", "action item", …) or if the item contains an injection-flagged line.
    *Widens; cost: some legitimate summaries are not stored.*
13. **`INSERT OR IGNORE` is not used.** SQLite's `OR IGNORE` also suppresses CHECK failures, which
    silently dropped an invalid evidence row and left a fact without evidence (found by test).
    Conflict-only `ON CONFLICT DO NOTHING` everywhere. *Widens.*

### Retrieval and rung 3

14. **Keyword channel of hybrid search is any-term** (stop words dropped, terms ORed, BM25), so
    natural-language questions match; the exact all-terms search stays for the UI. The recency
    prior is a multiplier `1 + 0.15 · 0.5^(age/14 d)` on the RRF score (plan: "add a recency
    prior, weight 0.15" — as an addend it would outweigh every channel). *Neutral.*
15. **Rung 3 sends only the masked image and a fixed instruction** — no title, OCR text or file
    name — and discards any answer that contains a masked value. VLM output stays plain text as in
    plan §16.2, with the observed transcription and the single `VISUAL:` interpretation kept
    apart. **Masking only hides what OCR detects**: on the dev machine Windows OCR skipped a
    card-number line entirely, which was therefore not masked. *Documented gap; mitigation (union
    with rung-1 UIA element boxes) not built yet.*

## Consequences

The plan (§7, §8, §9.2, §10.3, §16.2) and stages.md are updated in the same change. Items 7, 11,
12 and 15 are user-visible trade-offs to revisit with beta data.
