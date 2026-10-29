# CHAPTER 4|SYSTEM ARCHITECTURE AND DESIGN

## 4.1|Overview

Tessera is a Rust workspace of ten libraries behind a desktop shell, running as two cooperating operating-system processes plus the interface renderer. This chapter describes how the parts fit together and why the boundaries fall where they do. Implementation detail — the algorithms, the data structures, the specific code — is deferred to Chapter 5.

## 4.2|Design Principles

Four principles determined most of the structure.

**The model proposes, code disposes.** No guarantee offered to the user may depend on the language model behaving correctly. Wherever the model produces something, deterministic code checks it before it has any effect. This principle is why the extraction pipeline has four validation stages after the model call, and why the answer pipeline verifies each sentence individually.

**Fail closed.** When a check cannot reach a conclusion, the system takes the restrictive branch. A browser window whose private-browsing state cannot be read is treated as private and skipped. A focused element whose password status cannot be determined is treated as a password field and refused. A model output that is partly invalid is rejected in full rather than repaired, because a repaired output is one nobody specified.

**Make the dangerous thing structurally impossible.** Where a rule can be replaced by an arrangement in which breaking it is not expressible, it is. All network code lives in one library, and a build-time check fails the compilation if any other library can reach a network-capable dependency. Sensitive strings are wrapped in a type whose display implementation prints `<redacted>`, so a careless log statement cannot leak content. A fact cannot exist in the database without an evidence row, because a trigger deletes it when the last one is removed.

**Measure against criteria written beforehand.** Every numeric target in Table 3.2 was recorded before the corresponding code was written, and the evaluation harness is part of the repository.

## 4.3|High-Level Architecture

Figure 4.1 shows the system as a whole.

FIG:4.1

Three process boundaries divide the system. The **interface process** is the WebView2 runtime rendering a React application; it holds no data of its own and communicates only through a fixed set of typed commands. The **host process** contains all of the application's own logic: gating, capture, redaction, storage, retrieval, the pipeline, scheduling and supervision. The **inference worker** is an external `llama-server` executable confined to a Windows Job Object, reachable only over a loopback socket.

The arrangement is deliberately asymmetric in trust. The host process trusts neither of the other two. It validates everything arriving from the interface, and it treats everything arriving from the worker as an unverified proposal.

## 4.4|Module Decomposition

The host process is divided into ten libraries. The division follows the trust and platform boundaries rather than convenience: the libraries that implement security decisions are platform-neutral, so they can be tested exhaustively without a Windows desktop, and the one library that can open a network connection is isolated so that a tool can prove nothing else does.

Table 4.1 lists each library, what it is responsible for and its size in Rust source lines. The whole system is written in Rust, whose compile-time memory and thread-safety guarantees [26] matter here because the capture path handles untrusted text on background threads.

TBL:4.1

Three aspects of this decomposition are worth remarking on. First, `tsr-guard` contains the blocklist and redaction engines and has no Windows dependency at all, which is what allows its 355-case secrets corpus to run in ordinary continuous integration. Second, `tsr-infer` is the largest library because it carries four separable concerns — hardware discovery, process supervision, transport and scheduling — that nevertheless share a lifecycle. Third, `tsr-net` is small by design: it is the whole attack surface for outbound traffic, so it is kept small enough to read.

## 4.5|Process Architecture and Isolation

Figure 4.2 traces an interactive request from the interface to the hardware and back.

FIG:4.2

The inference worker is started through a specific sequence, each step of which closes a particular failure mode.

+ The child is created **suspended**, so that it cannot execute a single instruction before the remaining controls are in place.
+ Its **environment is cleared** and repopulated from a short allowlist (`SystemRoot`, `windir`, `TEMP`, `TMP`, `NUMBER_OF_PROCESSORS`). The inference server reads configuration from environment variables with an `LLAMA_ARG_` prefix, so an inherited variable could otherwise change its behaviour — for instance by enabling a feature or supplying a credential. This is not hypothetical: it was tested by setting such a variable in the parent and confirming that the child ignored it.
+ The child is **assigned to a Job Object** configured to kill its members when the last handle closes. Because that handle closes when the host process exits for any reason, an orphaned worker holding gigabytes of GPU memory cannot survive a crash.
+ The child is then **resumed**, binds a random high-numbered port on the loopback interface, and the host polls its health endpoint.
+ Before trusting that endpoint, the host **verifies the port's owner**: it queries the operating system's TCP table and confirms that the process identifier owning the listening socket is the child it created. Without this check, any other local process could race to bind the port and impersonate the worker.

A supervisor state machine then manages the worker's lifetime: health polling, hang detection, exponential backoff with jitter on restart, a restart budget so that a permanently broken configuration is reported rather than retried forever, and unloading the model after an idle period so that it does not occupy memory while the user is doing something else.

## 4.6|Data Acquisition Design

Figure 4.3 shows the acquisition path end to end.

FIG:4.3

Acquisition is organised as a sequence of gates followed by a three-rung ladder. The ordering matters: every gate that can reject a window runs before any text is read, and redaction runs before anything reaches a buffer that might later be written.

**Gates.** Capture begins paused on first run. Beyond that, the gatekeeper stops capture when the session is locked, when the machine is suspending, when the user has been idle, and when a full-screen Direct3D application or a presentation is in the foreground — the last of these so that capture cannot drop frames in a game or a presentation. On battery power the sampling interval is multiplied and the vision rung is disabled entirely.

**Blocklist.** Surviving windows are evaluated against process names, window titles and browser private-state indicators. Password managers and banking sites are blocked by default. A browser window whose private state cannot be determined is blocked. A deliberate refinement here is that generic words do not over-block: a non-browser window merely containing the word "password" in its title is still captured, because otherwise a developer's text editor would be invisible to the system; this behaviour is pinned by a dedicated test.

**Rung 1 — UI Automation.** The accessibility tree of the focused application is read on a dedicated thread with a 285-millisecond wall-clock deadline. Controls marked as password fields are excluded structurally rather than filtered afterwards. The deadline is the central design element here: accessibility calls are synchronous requests into another process and can block for seconds, so a worker thread performs the read and the caller abandons the result when the deadline expires. The abandoned read continues in the background and its result is discarded. In measurement across sixteen windows and nine applications, no read ever blocked the caller for more than 294 milliseconds, although the abandoned reads themselves ran for as long as 732 milliseconds.

**Rung 2 — Graphics capture with optical character recognition.** When rung 1 times out or returns too little, a single frame is taken into an in-memory bitmap and passed to the WinRT recognition engine. Because a bitmap has no structure, the bounding boxes of known password controls are blanked before recognition.

**Rung 3 — Vision model.** When recognition quality falls below a configurable threshold, the window is downscaled to at most 1280 pixels, encoded to a PNG in memory and sent to the vision-language model. At no point is an image written to disk.

**Change detection and sessions.** The resulting text is fingerprinted with a 64-bit SimHash. If the fingerprint is within a Hamming distance of three of the session's current fingerprint, only the session's end time is extended. Otherwise, only the lines the session has not already seen are appended, so that a chat window scrolled through repeatedly contributes each message once.

**Redaction, then storage.** Redaction runs on the text, the window title and the URL before any of them enters the session buffer. Sessions are flushed to the database as single items on the conditions described in Section 3.5. Pausing discards open sessions without storing them.

## 4.7|Hardware Backend Selection

FIG:4.4

The design goal is that the application binary contains no vendor code, and that the choice of backend is a run-time discovery rather than a build-time assumption. The six-step arbitration is shown in Figure 4.4.

Three aspects deserve emphasis. **Adapters are enumerated without loading a vendor runtime** — through DXGI on Windows and the kernel's device tree on Linux — because loading a vendor runtime on a machine with a mismatched driver is itself a common cause of failure. **Each candidate build is verified byte-for-byte** against a manifest compiled into the application, covering every file and refusing any extra file, because the acceleration code lives in a separate library beside the server executable and pinning only the executable would leave the interesting part unpinned. And **the probe is real**: each verified build is executed with a device-listing flag in a cleared environment, which initialises the backend for genuine and reports usable devices and free memory without loading any model.

The model is never altered to fit the hardware. If a tensor type is not known to be supported by a backend, that backend is skipped rather than attempted; if the model does not fit, the system offloads fewer layers or moves the vision encoder to the processor, but it does not re-quantise or substitute a different model. The supported backends and their verification status are given in Table 4.2.

TBL:4.2

## 4.8|Storage Design

Figure 4.5 shows the entities and their relationships, and Table 4.3 lists the tables each migration creates.

FIG:4.5

The schema is created by four forward-only migrations tracked in the database's own version counter, each applied inside a transaction together with its version bump so that a failure leaves the database at the previous version. A database written by a newer build is refused rather than guessed at.

TBL:4.3

Several decisions in the schema exist to enforce guarantees that would otherwise be left to application code.

**Provenance is mandatory.** A fact is meaningless without evidence, so `fact_evidence` carries the quotation, its character span in the item, and how it was matched — exactly, after normalisation, or fuzzily. A trigger deletes the fact when its last evidence row is removed, which means that deleting a captured item automatically removes any fact that depended solely on it.

**Enumerations and lengths are enforced by the database.** Fact kinds, statuses, confidences, acquisition rungs and job states are all constrained by `CHECK` clauses, as are text lengths. A bug in the pipeline therefore cannot store a fact in a state no one designed.

**Full-text indexes are kept in step by triggers, and deletions are honoured immediately.** Both full-text indexes are configured so that deleted terms are removed from the index at once rather than at the next merge, so that a deleted memory is not still discoverable through index pages.

**Forgetting is one-way.** When a user asks the system to forget a person, the suppression list stores only a keyed digest of the normalised identifier, salted per database. The list therefore cannot be read back into a set of names.

**Jobs are durable.** The scheduler itself is in memory; the `jobs` table is what survives a restart, so queued and running work is re-queued on the next start rather than lost.

## 4.9|Retrieval Design

Retrieval runs three channels and fuses them. The lexical channel runs BM25 over captured items; a second lexical channel runs over the facts table and maps each hit back to the items its evidence points into, so that a well-phrased fact can surface an item whose raw text matches poorly; the vector channel embeds the query and performs a brute-force cosine comparison against the stored int8 chunk vectors.

Two design choices are worth justifying. First, the vector search is **brute force** rather than an approximate index. At the scale of a single user's memory this is fast enough, and it removes an entire category of correctness problem — an approximate index that silently degrades as it grows — in exchange for a cost that is linear and predictable. Second, **filters are applied before fusion, identically on every channel**: a source must pass the *current* blocklist, not the blocklist that was in force when it was captured, and an item mentioning a suppressed identity is excluded everywhere. Applying the filter after fusion would allow a newly blocked source to influence ranking even if it were not shown.

Fusion uses Reciprocal Rank Fusion with the conventional constant of 60, after which a recency multiplier of 1 + 0.15 × 0.5^(age in days / 14) is applied. The multiplier is deliberately weak: it can reorder near-ties but cannot outweigh a channel's judgement. Ties break by newer item and then by lower identifier, so that the ordering is fully deterministic and therefore testable. If the vector channel is unavailable, the keyword results are returned alone and the diagnostics record that this happened, rather than silently returning a worse result.

## 4.10|Scheduling Design

Figure 4.6 illustrates what happens when the two collide.

FIG:4.6

One model instance serves interactive requests, background extraction and periodic summarisation, which are activities with very different latency requirements. The scheduler assigns three priorities: P0 for interactive work such as answering a question or writing a draft, P1 for near-real-time work such as extraction, and P2 for background work such as rollups.

Because the inference server processes one request at a time, a background job already streaming would otherwise delay an interactive one by many seconds. The scheduler therefore **preempts by dropping the connection**: closing the HTTP stream causes the server to abort generation immediately. The preempted job is re-queued *in its original position* with its attempt count unchanged, so that preemption is not treated as a failure, and partial output is discarded because nothing is committed until a job completes.

Three further rules prevent pathologies. A job may be preempted only a bounded number of times, after which it runs to completion, which prevents livelock. A background job that has waited beyond a threshold is promoted to compete at the next priority up, which prevents starvation. And a job key may be queued or running only once and is refused after completion, which makes duplicate submissions and retries idempotent — a job can never complete twice.

The scheduler is written as a pure state machine that owns no threads, never sleeps, and takes the current time as an argument. The same code therefore runs both in the threaded runtime and inside a discrete-event simulation used for evaluation, which is what makes its behaviour reproducible.

## 4.11|Privacy and Security Boundaries

Table 4.4 lists the trust boundaries, what crosses each one and the control applied there.

TBL:4.4

The boundary between captured text and the model prompt deserves particular attention, because it is where the injection problem lives. Captured text is escaped and placed inside explicit delimiters, and the prompt states that the delimited region is data. That alone would be a weak defence, since a sufficiently persuasive injection can talk its way past an instruction. The defence that actually carries the weight is the absence of capability: the model has no tools, cannot make requests, and its output is constrained by a grammar to a JSON object with a fixed set of fields. A successful injection can therefore influence what text is proposed, and nothing else. Sections 5.10 and 6.5 describe the additional classifier that attempts to stop even that, and measure how often it fails.

Logging is subject to a complementary discipline. Rather than filtering content out of log messages, the logging layer admits only messages and fields on an allowlist, and discards any field value that is not numeric, boolean or a short allowlisted identifier. A developer adding a log line with a user string therefore produces no output rather than a leak.

## 4.12|Error Handling and Recovery

Failures were categorised during design and each given a defined behaviour rather than being handled where they happened to arise.

A **worker crash or hang** is detected by the supervisor, which restarts with exponential backoff and a restart budget; in-flight jobs are re-queued and the interface reports a degraded state. A **backend that fails to start** causes the next candidate to be tried, with the reason recorded; the processor is always the last candidate. A **text read that exceeds its deadline** falls through to the next rung. A **model output that fails schema validation** is retried once and then the item is marked as failed, rather than being partially parsed. A **fact whose evidence cannot be located** is dropped with a counted reason. A **database that is newer than the build** causes a refusal to open. A **migration failure** leaves the database at its previous version because each migration shares a transaction with its version bump. Every one of these produces a content-free audit event, so that the diagnostics view can show what went wrong without showing what the user was doing.

## 4.13|User Interface Design

The interface is a single fixed-size transparent window pinned to the top of the monitor under the cursor, with three states: a compact pill, a peek showing a short summary, and an expanded panel containing the Today, Open Loops, Search, Timeline, Ask and Diagnostics views. Switching panels changes the window *region* rather than resizing the window, which avoids the visual artefacts that resizing a transparent layered window produces.

Two behaviours are design requirements rather than refinements. The window sets display affinity [23] to exclude itself from capture, on every visible window of the process, and reads the value back to confirm it took effect; if it did not, the interface says so rather than silently appearing in a screen share. And the interface does not steal focus: a peek appears without taking the foreground, and when a user-opened panel closes, focus returns to the application that had it, so that keystrokes never vanish into a hidden window.
