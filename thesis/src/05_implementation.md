# CHAPTER 5|IMPLEMENTATION AND METHODOLOGY

## 5.1|Development Methodology

The project was developed in numbered stages, each with written exit criteria fixed before the work began. Stage 0 established feasibility and tooling; Stage 1 built the foundation, capture and baseline guardrails; Stage 2 the understanding pipeline; Stage 3 the interface; Stages 4 to 6 the rollups, draft assistant and question answering; Stage 7 a security audit. Stages 8 to 10 — meeting capture, closed beta and release engineering — were gated behind the release criteria and are not implemented.

Two practices from this method are worth describing because they shaped the result.

**Decisions were recorded, including the uncomfortable ones.** Ten architecture decision records document the significant choices, each stating the context, the options considered and the consequences. Two of them record unresolved problems rather than solutions: one records an outbound network connection made by the WebView2 runtime that the project could not switch off, and one records that the automatic updater cannot ship because of unpatched advisories in a transitive dependency. Writing these down as open items, rather than quietly narrowing the criterion they violate, is what allows Chapter 6 to report a failure instead of a redefinition.

Table 5.1 lists the ten records and what each settles.

TBL:5.1

**Verification was separated from implementation.** Each stage produced a verification report labelling every measurement as REAL, SIMULATED or UNVERIFIED. A claim measured with a stand-in model is never presented as if it had been measured with a real one. This distinction is preserved throughout Chapter 6.

## 5.2|Repository Structure and Build

The workspace is a Cargo monorepo. Dependencies whose feature flags carry privacy consequences are pinned to exact versions rather than compatible ranges, and two of these pins are themselves security controls.

The embedding library's default features would pull in a client for a model hub — an HTTP stack — and a build-time download of the inference runtime. Both are disabled, and the runtime is loaded dynamically from a local file instead. The ONNX Runtime binding is configured to demand API level 31, which causes it to refuse any runtime library older than version 1.31. This matters because Windows ships its own copy of that library in the system directory, at version 1.17 on the development machine; without the API-level requirement the system copy would be silently accepted, and the "pinned runtime" guarantee would be false. A test confirms that the system copy is never mapped into the process.

```
cargo test --workspace -- --include-ignored     # 375 test functions
python tools/check_network_deps.py --locked     # dependency boundary
cargo deny check                                # licences, bans, advisories
python evals/validate.py                        # evaluation set integrity
pnpm build:ui && cargo build -p tessara --features custom-protocol
```

## 5.3|Core Primitives and Content-Free Logging

`tsr-core` defines `Sensitive<T>`, a wrapper for any string that may contain user content. Its `Debug` and `Display` implementations print `<redacted>`. The practical effect is that the ordinary ways content escapes — a debug print during development, a value interpolated into an error message, a panic message containing a captured string — all produce the placeholder instead.

The logging layer takes the complementary approach of an allowlist rather than a filter. A `tracing` subscriber emits one JSON line per event and keeps the message **only if it is one of a fixed, reviewed set of strings**; otherwise the line records a null message and names the call site so that the offending log statement can be found and fixed. Field values survive only if the field name is on a second list and the value is a number, a boolean, or a string of at most 160 printable ASCII characters. Everything else is dropped and counted.

This design means that adding a new log message is a reviewed change to a single file. A workspace-wide test parses every `tracing::` call in the repository and fails if any of them uses a message or field name that is not on the lists, so the allowlist cannot drift out of date.

## 5.4|Capture Implementation

The capture core is written as a platform-neutral state machine. The operating system is behind a `Platform` trait, storage is behind a `Sink` trait, and the current time is passed in as an argument. The consequence is that the whole of the gate logic, the ladder ordering, the blocklist interaction, session merging and flushing can be tested with simulated windows and a real encrypted database, with no desktop session and no clock, which is how the behaviour described in Section 6.3 is verified.

**The deadline worker.** Accessibility reads are the one part of capture that cannot be made safe by structure alone, because the call blocks inside another process. The implementation runs reads on a dedicated thread initialised for the multi-threaded apartment and has the caller wait with a 285-millisecond deadline. When the deadline expires the caller proceeds without a result and the worker's eventual answer is discarded. The measured consequence is that the capture loop never stalled beyond 294 milliseconds even against applications whose underlying reads took 732 milliseconds.

**Change detection.** SimHash is computed over overlapping groups of three consecutive words. An important implementation detail is that the hash function is a hand-written FNV-1a rather than the standard library's hasher, because the standard hasher is randomly seeded per process and its algorithm is not guaranteed stable across versions. The fingerprint is stored in the database and compared across runs, so it must be reproducible.

**Sessions.** A session is identified by the process, the top-level window and the document — the URL for a browser, and otherwise the redacted title with volatile counters such as an unread count removed, so that a changing message count does not look like a new document. When a snapshot's fingerprint matches, only the end time advances. When it does not, only the lines the session has not already recorded are appended.

## 5.5|Redaction Implementation

Each detector returns byte spans; overlapping spans are merged; each surviving span is replaced by a marker naming the kind of secret. Only per-kind counts leave the module — never the matched values, which is why a redaction count can appear in the database and in diagnostics without leaking anything.

Fifteen kinds are detected: private keys, JSON web tokens, API keys, payment cards, Aadhaar numbers, PAN, passport and voter identity numbers, bank account numbers, IFSC codes, UPI identifiers, one-time codes, passwords, generic secrets, and optionally phone numbers and email addresses. The last two are retained by default because identity resolution needs them, and can be redacted by configuration.

The design decision that determines the false-positive rate is that **numbers which could be anything are redacted only when a checksum agrees**. A sixteen-digit sequence is redacted as a payment card only if it satisfies the Luhn algorithm *and* carries a recognised network prefix. A twelve-digit sequence is redacted as an Aadhaar number only if it satisfies the Verhoeff check. Keys and tokens are matched by their published prefixes; an unprefixed high-entropy string is redacted only when it appears next to a word such as "key", "token" or "secret".

One defect found during the security audit is worth recording because it illustrates why a single pass is insufficient: a second one-time code falling inside the keyword window of the first survived redaction, because replacing the first changed the offsets. Redaction now runs repeatedly until the output stops changing.

## 5.6|Storage Implementation

The data directory holds two files: the encrypted database and a key file consisting of a four-byte magic value, a version byte and a data-protection blob. A key is created only when **neither** file exists. If the database is present but the key file is missing or unreadable, opening fails rather than generating a new key — because generating one would make the existing database permanently unreadable while appearing to succeed.

The database is opened with security-relevant pragmas: memory security on, secure deletion on, foreign keys enforced, temporary tables in memory, and write-ahead logging. SQLCipher's own logging is disabled, which is verified by a test that opens a database with the wrong key in a child process and asserts that nothing is written to standard error — a plausible leak path, since a cipher error message can contain fragments of the file.

## 5.7|Inference Sidecar Implementation

The loopback client is a minimal HTTP/1.1 implementation written directly on the standard library's networking, with no HTTP crate. This is not an exercise in reinvention: including a general HTTP client would put a network-capable dependency into the graph and break the compile-time boundary described in Section 5.15. The client is restricted to the loopback address, speaks only the few verbs the inference server needs, and supports the streaming responses the scheduler must be able to abort.

The GGUF parser reads only the header — the format version, architecture, tensor types and sizes — without touching tensor data, so inspecting a 2.4-gigabyte model file costs a few kilobytes of reads.

## 5.8|Scheduler Implementation

The scheduler is a pure state machine: it owns no threads, never sleeps, and receives the current time as a parameter. Two consumers drive it. The threaded runtime calls it from the application; a discrete-event simulation calls it from the evaluation harness to replay a synthetic backlog deterministically. Because both use the same code, a scheduling behaviour verified in simulation is the behaviour the application exhibits.

Each job carries a *key*, such as `extract:item:42`. A key may be queued or running only once, and a key that has completed is refused until it is explicitly forgotten. Retries and duplicate submissions therefore cannot cause a job to complete twice — which matters because completing an extraction twice would write a duplicate set of facts.

Preemption is implemented by cancelling the HTTP stream, which the inference server observes as a client disconnection and responds to by aborting generation. The preempted job returns to its original queue position with its attempt count unchanged, and any partial output is discarded.

## 5.9|Extraction and Deterministic Validation

Figure 5.1 shows the complete path from flushed text to a stored fact.

FIG:5.1

Extraction is the clearest expression of the principle in Section 4.2, so the stages are described in order.

**Prompting.** The flushed session text — already redacted — is escaped and wrapped in explicit delimiters. The model is given a JSON schema and constrained by a grammar so that the output cannot be prose, cannot be fenced in a code block and cannot contain unknown fields.

**Schema parsing.** The parser accepts exactly the schema: one JSON object, no unknown fields, enumerated values from fixed lists, length limits counted in characters, no control characters, no empty strings. Any violation rejects the whole output. It is deliberately **never repaired into something valid-looking**, because a repaired output is one that no one specified and whose relationship to the source is unknown.

**Evidence verification.** Each fact must carry a quotation. The verifier tries an exact substring match, then a match after normalising whitespace and punctuation, then a fuzzy match over word sets in a sliding window using the Dice coefficient with a threshold of 0.9 on quotations of at least four words. Quotations shorter than two words or eight characters are refused as too weak to support anything. Model text that is an explanation, a paraphrase or an invention therefore never becomes evidence. The character span of the successful match is stored, which is what makes a citation chip able to open the item and highlight the right text.

**Injection classification.** Described separately in Section 5.10.

**Second redaction.** Redaction runs again on the model's output, because the model may reproduce a secret in a different form from the one the first pass matched.

**Resolution.** The date resolver converts the phrase the model copied into a timestamp *against the item's capture time, never against the current time* — so reprocessing an old item cannot silently change what "tomorrow" meant. Inputs are explicit: the reference time with its zone, the user's numeric date order taken from the Windows locale, and the end-of-day hour. Results are one of resolved, ambiguous, unresolved or invalid; an ambiguous result — a day-month ordering that could go either way, "next Friday", a weekday that does not match the stated date, or a time that falls in a daylight-saving gap — is stored with its preferred reading flagged as ambiguous and is never presented as certain.

The people resolver is deliberately conservative. An email address or handle identifies a person exactly. A bare name links to an existing person only if exactly one person has that name and no identifier contradicts it; the same name with a different email address is a different person. If two people share the name, the fact simply keeps the name as text. Near matches are *suggested* for the user to confirm and never merged automatically.

**Deduplication and the atomic write.** A fact sufficiently similar to an existing one is merged and its last-seen time updated. Everything — the facts, their evidence rows and the item's summary — is written in a single transaction.

## 5.10|The Injection Guard

The guard classifies each proposed fact from **the source text surrounding its evidence**, not from the model's wording. This is the key design decision: a model that has been successfully injected will word its output innocuously, so judging the output alone would be judging the attacker's choice of words.

Four classes are produced. *Injection* — text aimed at an assistant or model, or asking to exfiltrate, delete or reveal data, or to edit the system's own memory — rejects the fact outright. *Quoted* — an instruction inside a quotation, a forwarded block or a code block — permits storage but never as a task. *Ambiguous* — an imperative addressed to "me" with no sign that it is addressed to the user — is stored with a status that keeps it out of task lists until the user confirms it. *Genuine* covers everything else.

The classifier is conservative by design, to the point that a legitimate note which merely quotes an injection phrase is also rejected; such rejections are counted rather than silent. The same detector screens generated summaries and extracted person names.

The guard is a mitigation, not a solution, and Section 6.5 measures precisely how incomplete it is.

## 5.11|Retrieval and Grounded Answering

FIG:5.2

Chunking is deterministic and provenance-preserving: windows of at most 380 words overlapping by 48, cut at a paragraph or sentence boundary when one is nearby, each recorded as a character span of the item so that a retrieved chunk always leads back to exact source text. The chunker configuration is versioned and the version is stored with every vector, so that changing the chunking does not silently mix incompatible vectors. Vectors are stored as int8 values with a single floating-point scale, which quarters the storage relative to 32-bit floats.

Answering proceeds as in Figure 5.2. The model receives the snippets and must produce JSON in which each sentence names the snippet it came from. Sentences are parsed incrementally as they stream, so the first verified sentence can be displayed before the answer is complete. Each sentence is then checked against the snippet it cites using the same claim machinery as the draft fact checker. A sentence that does not verify is **hidden and counted**, not shown with a warning. If nothing verifies, the system says it could not find the answer and offers the three best sources for the user to read. The measured outcome of this policy is zero fabricated citations, at the cost of a verification rate of 83.7 per cent against a target of 90.

## 5.12|Draft Assistant

The draft assistant is the only part of the system that writes into another application, so it carries the most safety machinery.

Before generation, the focused element's runtime identity is recorded along with whether it is editable and whether it is a password field; an unknown password state counts as a password and refuses. Thread context — the last two thousand characters or so, redacted — is gathered along with relevant memory. Generation runs at interactive priority with a single retry on invalid output.

The fact checker then runs **independently of the model**. It extracts checkable claims — email addresses, URLs, file names, dates, times, weekdays, relative days, amounts, numbers and capitalised names — and marks a claim supported only when its normalised form occurs in the context. Normalisation is what makes "28 Oct" match "28 October", "1,000,000" match "1000000" and "5 pm" match "5pm". Bracketed placeholders are never treated as claims. On top of claim checking, the checker flags new commitments the draft would create, sentences that look like repeated injected text, and any secret.

Insertion requires an approval token. On insertion the system re-verifies that the focus target is still the same element, saves **every** clipboard format rather than only text, writes the draft marked with the formats that exclude it from clipboard history and cloud synchronisation, sends the paste keystroke, and restores the clipboard after a short delay. The system never sends Enter.

Two defects found during this stage are instructive. The clipboard restore never ran, because the clipboard sequence number was read while the clipboard was still open and closing it advances the number, so the code always concluded that someone else had copied in the meantime. During the test that revealed this, the development machine's clipboard contents were lost and could not be recovered. Separately, saving only text would have refused almost every real clipboard, since real clipboards usually carry HTML or rich text alongside the plain text.

## 5.13|Rollups, Brief and Open Loops

Rollups are produced at hour, day, week and month level with local-time boundaries stored as UTC milliseconds. What is due is **derived from the store each time** rather than tracked in a separate queue, which makes the schedule durable by construction: an hour with activity becomes due five minutes after it closes, a day once it ends, and a parent waits until every child with activity has a fresh summary. Idle hours get no summary at all.

Rollup output is validated by code in the same spirit as extraction: strict schema, highlight identifiers must correspond to entries of that period with invalid ones stripped, word limits per level, redaction, and rejection of any sentence that looks injected or that instructs the reader. If no model is available, or the model fails twice, **an extractive summary is written by code** so that every active period still has a rollup; it is regenerated with the model only if it later goes stale. Writing a summary marks the summaries above it stale, and deleting an item marks every summary that cited it stale.

The morning brief is assembled entirely by code from the store — overdue and due-today loops, open commitments in both directions, questions the user is waiting on, and yesterday's already-validated summary. The opening line is generated from counts rather than by the model, so the brief never waits for inference and contains no unvalidated model text.

Open loops are never closed automatically. Completion evidence moves a fact to a *proposed* state that the user confirms. This is enforced rather than merely intended: a test scans the source and fails if any code path other than the approval-token command writes a closed status.

## 5.14|Desktop Shell and Interface

The shell exposes a fixed set of typed commands. Inputs are validated for length, enumeration membership, identifier validity and token validity, and errors return a code rather than a message containing content. The content security policy permits only the application's own origin with no inline script and no evaluation, navigation is locked, the JavaScript prototype chain is frozen, developer tools are absent from release builds, and exactly one capability — event listening — is granted.

Status-changing operations use a two-step approval token: the interface asks for a token, the user confirms, and the token is single-use. This is what prevents an interface defect, or an injected instruction that reached the interface, from closing a user's commitments.

## 5.15|Network Boundary and Model Onboarding

The boundary is enforced at build time. A Python script walks the full dependency graph produced by Cargo and fails the build if any library other than `tsr-net` can reach a network-capable crate. It is run for both the default and the updater feature sets, and — importantly — it is itself tested against deliberately broken fixtures to confirm that it rejects them, so a checker that silently passed everything would be caught.

Downloads require a typed approval value that can be constructed only from an explicit user action. Files are written to a partial file with an accompanying metadata record so that an interrupted download resumes, and the final file appears only by atomic rename after its SHA-256 digest matches a pinned manifest. Since the security audit, model files are also digest-checked on first use and whenever their size or modification time changes, so that a file substituted after installation is detected.

## 5.16|Configuration

Configuration lives in a single JSON file in the application's local data directory; the full reference appears in Appendix C. The defaults are chosen so that an unmodified installation is the safe one: automatic backend selection, full offload when memory allows, a ten-minute idle unload, a five-second active sampling cadence and a ninety-day retention limit.

## 5.17|Summary

The implementation follows one pattern repeatedly: identify the point at which an unverified claim or an unsafe action could enter the system, and put a deterministic check there that does not depend on the model. Chapter 6 measures how well that pattern worked.
