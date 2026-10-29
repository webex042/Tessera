# CHAPTER 7|DISCUSSION, LIMITATIONS AND FUTURE SCOPE

## 7.1|What the Implementation Achieves

The central claim this work can support is narrower than the one it set out to make, and more interesting for being narrow.

A desktop application *can* run a four-billion-parameter vision-language model across unrelated GPU vendors without containing a line of any vendor's code, by treating each backend as an externally pinned artefact and discovering the hardware at run time. It *can* read text from other applications without writing a single screen image to disk and without blocking its own capture loop beyond 300 milliseconds, even against applications that take three-quarters of a second to answer. It *can* remove every secret in a 355-case corpus while flagging fewer than one clean line in two hundred, provided the detectors validate checksums rather than matching digit patterns. It *can* guarantee that no stored fact exists without a verbatim source span, and the measured hallucination rate of such a system is zero rather than small. And it *can* hide its own interface completely from two independent screen-capture paths.

What it cannot yet do, on the hardware available and with a model of this size, is be accurate or fast enough to be pleasant. Extraction misses too much and over-produces trivia. Answers are right two times in three where the criterion asked for four in five. An answer takes twenty-five seconds where the criterion asked for two and a half. And one process in the application's own process tree — the browser runtime that renders the interface — makes a network connection that the project could not switch off and therefore could not claim did not exist.

The value of the result is in that separation. It was not obvious in advance which guarantees would depend on the model and which would not. The evaluation answers that question empirically: **every criterion enforced by deterministic code was met, and every criterion that depended on model quality, hardware throughput, or a third-party component was missed.** That is a useful finding for anyone designing a similar system, because it says where to put the engineering effort — and where not to bother asking the model to behave.

## 7.2|Objectives Met

Table 7.1 revisits each objective from Section 1.4 with the evidence gathered.

TBL:7.1

Seven of the eleven objectives are achieved, three are partially achieved, and one is not achieved as stated.

## 7.3|Design Trade-Offs

**Strictness against recall.** Rejecting an entire model output on any schema violation, rather than repairing it, costs facts that a lenient parser would have kept. The compensating benefit is that nothing stored is of unknown provenance. The evaluation shows the cost concretely: before the key-order defect was found, fifteen of fifty-eight items produced nothing at all. A lenient parser would have salvaged some of them and would also have salvaged whatever the broken grammar had caused the model to emit.

**Conservatism against usefulness in the injection guard.** The guard rejects a legitimate note that merely quotes an injection phrase. On the corpus, three to four genuine tasks per run were missed for this reason. This is the right direction to err for a system that stores things the user will later trust, but it is a real cost, and it means the guard's precision cannot be raised without lowering its recall against attacks.

**Brute-force vector search against scale.** Comparing a query against every stored chunk is linear in the size of the memory. An approximate index would be faster and would introduce a failure mode — silent degradation of recall as the index grows — that is difficult to detect in production. At single-user scale the linear cost is acceptable, and this trade-off would have to be revisited if the retention period were extended far beyond the ninety-day default.

**Preemption by connection drop against wasted work.** Dropping a streaming connection to free the model for an interactive request discards whatever the background job had generated. The alternative — letting background jobs finish — would make interactive requests wait many seconds behind them. On an integrated GPU, where generation is slow, the waste is substantial; on faster hardware it would be smaller.

**An external inference process against latency and complexity.** Process isolation costs a start-up delay on the first request after an idle period, a transport hop for every token, and a substantial amount of supervision code. In exchange, a driver reset cannot take the application down. Given that out-of-memory resets are routine on 6 GB consumer cards, this was not a close decision.

**Capture starts paused.** A system that captures nothing until the user turns it on is less impressive in a demonstration and much easier to defend. The same reasoning produced the fail-closed behaviour on unknown private-browsing states, which certainly loses some legitimate captures.

## 7.4|Technical Limitations

**Model quality at this size.** The decisive limitation is the model. Extraction F1 of 0.533 is driven by a precision of 0.21 on `note` facts: the model produces a large volume of observations that a human would not record. The right response is not a larger model but a narrower task — the evaluation suggests that restricting extraction to tasks and commitments, where F1 is 0.68, would produce a system that is useful now, with notes left to a later prompt version.

**Latency on integrated graphics.** Twenty-five seconds to an answer is not usable. This is a memory-bandwidth problem rather than an algorithmic one, and the project's own target hardware was never available to test the hypothesis that a discrete GPU closes the gap. That hypothesis remains untested and should not be assumed.

**The embedding channel is unexercised.** The chunker, the vector index, the int8 encoding and the fusion logic are all implemented and unit-tested, but no embedding model was run end to end on the evaluation machine. Hybrid retrieval was therefore measured with its lexical channels only, and the contribution of the vector channel to answer quality is unknown.

**Thin application coverage.** Text acquisition was verified against seven of twenty target applications. The remainder were either not installed or required accounts whose contents could not be exposed to a test. The criterion of sixteen of twenty is unverified rather than failed, which is a weaker but more honest statement.

**No long-run data.** The eight-hour resource run, the seven-day stability run and the twenty-four-hour network and canary runs have not been performed. Memory leaks, handle leaks and slow index degradation would most likely appear in exactly those runs.

## 7.5|Security and Privacy Limitations

**Injected text can still reach the user's task list.** On the untouched half of the attack corpus, twelve of thirty-nine attacks produced an entry the user would see. None caused an action, revealed a secret or modified anything beyond the expected table — the architecture holds — but a user who trusts their task list could still be socially engineered through it. The weakest categories are social engineering phrased as ordinary correspondence, memory poisoning, delayed triggers and multilingual attacks.

**The WebView2 connection.** One process in the tree makes an outbound HTTPS connection per run. The application's own code makes none, but the user experiences a process tree, not a crate graph. Until this is resolved, the honest statement is the one the project's privacy documentation makes: nothing leaves the machine from Tessera's own code, and one connection from the interface runtime is not under its control.

**Memory outside the application's control.** Decrypted text exists in process memory while the application runs. If Windows pages that memory out, it may be written to the page file or the hibernation file. Neither was scanned, as that requires elevated privileges. Malware running as the same user, or physical access to an unlocked machine, defeats the design entirely — the data-protection key is bound to the user account, so an attacker who has the account has the data.

**No coverage-guided fuzzing.** Randomised testing with twenty thousand inputs per target found two real defects, but coverage-guided fuzzing, which explores systematically rather than randomly, could not be run because it requires a toolchain that could not be installed under the project's network constraints.

**The guard's generalisation estimate will expire.** The 62 per cent figure is honest only while the held-out half remains untouched. Any further tuning against it would require a third corpus written by someone who has not seen the guard.

## 7.6|Platform and Hardware Constraints

Capture is Windows-only, because it depends on UI Automation, Windows Graphics Capture, WinRT recognition, Job Objects, display affinity and the Windows data-protection interface. The storage, retrieval, inference and pipeline libraries are platform-neutral and compile without Windows, so a port would mean replacing the acquisition and key-protection layers rather than rewriting the system.

Five of the seven backend families are verified only through simulated adapter reports. The arbitration logic is tested; the vendors' drivers are not.

## 7.7|Unresolved Issues

Four questions remain open and require either a decision or hardware that was not available:

+ **The WebView2 connection.** Identify the endpoint with an elevated packet capture; if it is switchable, switch it off; if it is not, decide between a native interface and an explicit, documented narrowing of the criterion.
+ **Extraction precision.** Decide whether to suppress low-value note facts in the prompt, narrow the schema, or accept a tasks-and-commitments-only product for the first release.
+ **Hardware.** Re-run the full evaluation on a discrete GPU before concluding anything about achievable latency.
+ **A third injection corpus.** Needed before any further guard tuning can be measured honestly.

## 7.8|Future Scope

**Immediate — closing the current gaps.** Re-run the held-out extraction and answering evaluations on discrete hardware. Curb low-value note facts, which is the precision driver, rather than only fixing output formats. Commission a third, unseen injection corpus. Complete the outstanding long-duration runs and the live checks listed in Section 6.13.

**Near term — completing the specified system.** Meeting capture and live translation were specified and not built: audio held in memory only, per-meeting consent, local speech recognition and a permissively licensed translation model. Beyond that, a closed beta on varied hardware is the only way to obtain the backend verification that simulated adapters cannot provide, and release engineering — a signed installer, a software bill of materials per release, and an opt-in signed updater once the advisory blocking it is resolved — remains outstanding.

**Longer term — research directions.** Three are suggested directly by the results. *Speculative decoding* with a small draft model is the most promising route to acceptable latency on memory-bandwidth-limited integrated graphics, because it trades arithmetic, which such hardware has, for memory traffic, which it lacks. *Task-specific adaptation* of a small model on the extraction schema may recover more accuracy per byte than moving to a larger general model. And the broader question raised by Section 6.5 — how to measure a prompt-injection defence honestly, given that any corpus becomes a tuning set the moment it is used — deserves attention in its own right; the tuned-and-held-out split used here is a minimum, not a solution.

**Cross-platform support.** Abstracting acquisition behind the existing `Platform` trait would allow a Linux implementation using the desktop portal interfaces and a macOS implementation using ScreenCaptureKit, reusing the storage, retrieval and pipeline layers unchanged.
