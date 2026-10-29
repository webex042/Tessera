# CHAPTER 6|TESTING, EVALUATION AND RESULTS

## 6.1|Evaluation Methodology

Every target reported here was written down before the corresponding feature was built, and no target was changed after a measurement. Results are labelled consistently:

- **REAL** — measured against the real operating system, real processes, real hardware or the real language model.
- **SIMULATED** — measured with a stand-in for the model, the inference server or the embedder. A simulated result tests the surrounding code, not the model.
- **UNVERIFIED** — not measured. An unverified item is reported as unverified and never inferred from a related result.

Where a defence was tuned against part of a corpus, the untouched part is reported separately and treated as the honest estimate. Where a figure improved after a bug fix, both figures are given.

## 6.2|Test Environment

Table 6.1 records the machine on which every measurement in this chapter was taken.

TBL:6.1

The single most important fact about this environment is that the reference machine named in the plan — an NVIDIA RTX 4050 with 6 GB of dedicated memory — was not available. The available graphics processor is integrated, shares system memory with the processor, and has far lower memory bandwidth. Every latency figure in this chapter is therefore a lower bound on what the system can achieve, and the accuracy figures, which depend on the model rather than on throughput, are unaffected.

## 6.3|Functional and Structural Testing

Table 6.2 summarises the test and evaluation assets.

TBL:6.2

Testing is organised around the trait boundaries described in Section 5.4. Because the capture core takes the operating system, storage and time as parameters, the gate logic, ladder ordering, blocklist interaction, session merging and flush conditions are all exercised against simulated windows and a **real encrypted database**, with no desktop session required. Seventeen tests are marked as requiring a real desktop, real hardware or real binaries and are run separately.

Selected results that bear on the requirements:

- **Deadline compliance (REAL).** Across sixteen windows and nine applications, the maximum time an accessibility read blocked the caller was 294 milliseconds against a 300-millisecond requirement. Microsoft Word, PowerPoint and File Explorer all hit the deadline and were served by the second rung; their abandoned reads ran on for between 290 and 732 milliseconds in the background, which is the precise failure the deadline exists to prevent.
- **Blocklist behaviour (REAL and simulated).** Private-state detection was verified against real Chrome and Edge windows in both normal and private modes; an unreadable state produces a block. The complementary test — that an ordinary non-browser window merely titled with the word "password" is still captured — also passes, confirming that the rule does not over-block.
- **Password exclusion (REAL, partial).** In browser fixtures, a canary value placed in a password input never appeared in accessibility or recognised text. The equivalent test against a native Windows Forms or WPF application has **not** been performed and remains an outstanding item.
- **Encryption (REAL).** A database opened with a wrong or zero key is rejected; a hexadecimal scan finds no plaintext header and no canary string in either the database or its write-ahead log; a wrong-key open writes nothing to standard error.
- **Randomised robustness.** Twenty thousand generated inputs per target were run against the redaction engine, the date resolver and the text normaliser, and three thousand random chunkings against each streaming parser. This found two genuine defects — a panic in the date resolver on extreme offsets, and the non-idempotent redaction described in Section 5.5 — both fixed with regression tests. Coverage-guided fuzzing was not performed, as it requires a nightly toolchain that could not be installed under the project's network constraints.

## 6.4|Privacy and Security Evaluation

### 6.4.1|Redaction

Table 6.3 gives the redaction results.

TBL:6.3

The corpus was expanded during the security audit from 290 to 355 cases by adding Aadhaar virtual identifiers, passport numbers and voter identity numbers. On first run the sixty-five new cases **leaked forty-eight of them**, because no detector existed for those formats. Detectors were added and the redaction version incremented. This is reported here because it is the clearest available evidence for a general point: a redaction engine is only as good as the corpus it has been tested against, and a corpus that is not extended will certify a false sense of completeness.

### 6.4.2|Plaintext on Disk

A canary string was placed in a captured item, a window title, a stored fact, a summary and the open interface panel, and the application was run for sixty seconds. A scan of 285 files totalling 32 megabytes across the data directory, the WebView2 profile, the temporary directory and the crash-dump folders found **zero occurrences**. A positive control placed deliberately in a plain file was found, confirming that the scan worked.

Two caveats apply. The release criterion asks for a twenty-four-hour run, which has not been performed. And the Windows page file and hibernation file could not be scanned, as that requires elevated privileges; if the operating system swaps a page containing decrypted text, that text could in principle be written there. This is recorded as a residual risk in Section 7.5 rather than claimed as covered.

### 6.4.3|Network Behaviour

This is the clearest failure in the evaluation and is reported in full.

A ten-minute monitored run observed the complete process tree. The application process made **zero** outbound connections, and the inference worker was never seen outside the loopback interface. However, the Microsoft WebView2 process that renders the interface opened **one HTTPS connection to a Microsoft address** approximately one minute after start, in each of several runs.

Every mitigation available to the application had already been applied: the framework's defaults were preserved, the reputation service was disabled and the setting read back to confirm it, background-networking switches were set, proxy discovery was disabled, navigation was locked, and the runtime's environment variables were cleared. The connection does not appear in the browser engine's own network log, and the host name was absent from the system's resolver cache, which indicates that it originates in code outside the engine's network service — most plausibly a diagnostic client, although this is **not confirmed**.

The release criterion states zero connections from the application's processes to anything other than the loopback address. As measured, it fails. The project recorded this as an open decision rather than narrowing the criterion, and the privacy documentation states plainly that "nothing leaves your PC" cannot be claimed for the interface process until it is resolved. The options under consideration are to identify the endpoint with an elevated packet capture and disable it if it is switchable, or to replace the always-present interface with a native implementation and use the web runtime only for windows the user opens deliberately.

### 6.4.4|Dependency and Supply-Chain Review

The dependency boundary check passes for both the default and the updater configurations, and the checker itself was validated against deliberately broken fixtures. Licence and banned-crate checks pass; all dependencies are permissive, with one weak-copyleft licence present. Advisory checking passes against an advisory database cached offline; a fresh fetch and a Node package audit both require network access and were not performed. The automatic updater is compiled out of every build because a transitive compression library carries three unpatched advisories.

## 6.5|Injection Robustness

This evaluation is the most methodologically careful part of the project, because it is the easiest to get wrong. A defence tested only against the attacks it was built from will always appear to work. Figure 6.1 and Table 6.4 therefore separate the attacks used for tuning from those that were not.

FIG:6.1

The corpus contains 152 cases across thirty-eight categories — hidden text, fake system prompts, schema override attempts, store poisoning, delayed triggers, multilingual and transliterated attacks, attacks arriving through recognised text, attacks inside window titles, and five genuine controls that must *not* be blocked. Seventy-eight cases were written before any run of the defence against them. The corpus was then split in half: one half was used to tune the guard rules, and **the other half was never looked at during tuning**.

TBL:6.4

The pattern is unambiguous. Against the original set the defence is close to complete; against the tuned half it is near-perfect, which is expected and means nothing on its own; and against the untouched half it stops roughly 62 per cent of attacks when the real model is used. The gap between 100 per cent and 62 per cent is the measurement of how much the earlier numbers were worth.

The twelve held-out failures with the real model share recognisable styles: social engineering phrased as ordinary correspondence, such as a fake password-reset request or a payroll account change; attempts to poison stored memory; delayed-trigger variants; quoted phishing; and injections placed in window titles. Multilingual cases were among the weakest categories. **They have deliberately been left unfixed**, so that the 62 per cent figure continues to mean something; fixing them would turn the held-out half into a second tuned half, and a genuine estimate would then require a third corpus.

What holds regardless of the guard is the more important result. In all 152 cases:

- no injected instruction caused an action, because the model has no tools;
- no secret was revealed in clear text;
- no table other than the expected one was written;
- the delimited content block could not be closed from inside;
- exactly one model call was made per case.

The failures are therefore confined to a single consequence: text that the user would see in their task list which they should not trust — for instance a phishing entry asking the user to share an identity number, or a fabricated instruction to transfer money. This is a real harm and is not minimised here, but it is bounded, and the bound comes from the architecture rather than from the guard.

Five cases failed with a transport timeout during a run that the machine interrupted twice by entering standby. They are counted as failures and were not retried. One control case passed in an earlier run and failed in this one, which is a reminder that the model's output varies between runs even at the same settings.

## 6.6|Extraction Accuracy

Table 6.5 gives the aggregate result.

TBL:6.5

FIG:6.2

The headline result is a failure on quality and a success on integrity. The system extracts fewer of the right facts than the target requires, but **everything it does store is traceable**: of 97 stored facts, zero carried evidence that was not present in the source, and zero rows violated the schema.

The breakdown by kind in Figure 6.2 locates the problem. Task and commitment extraction — the facts that actually matter for a memory system — reaches an F1 of 0.68, just under the 0.70 gate. Event extraction has high recall and poor precision. The category that destroys the aggregate is `note`: precision 0.21 against 23 false positives, because the model generates a large number of low-value observations that the gold set does not consider worth recording. Decisions are the opposite failure: every decision the model produced was correct, but it found only a quarter of them.

**The key-order defect.** An earlier run of the same evaluation scored 0.507 with fifteen of fifty-eight items lost entirely to invalid JSON, and took five hours and forty-eight minutes. Investigation found that the JSON schema was being serialised to the model with its keys in alphabetical order, because the serialisation library's default behaviour sorts map keys. Since the grammar forces the model to emit fields in schema order, this meant the model had to commit to fields before it had produced the content they described. Restoring the declared key order removed every invalid-JSON failure, raised recall from 44.6 to 57.8 per cent and cut the run to roughly fifty minutes — but *lowered* precision from 58.7 to 49.5 per cent, because the model now successfully produced the low-value notes it had previously failed to emit. Net F1 improved from 0.507 to 0.533, still short of the gate.

This defect is worth recording as a general lesson: a grammar-constrained model is sensitive to field ordering in a way that is invisible in the schema definition and easy to introduce accidentally through a serialisation default.

Rejections were counted by reason: five facts had no evidence, four cited a person not present in the source, three had evidence too short to be meaningful, two contained an unsupported number, and eleven were rejected by the injection guard across its categories. Every one of these is a fact the user never sees — the intended behaviour.

## 6.7|Question Answering

Table 6.6 gives the results.

TBL:6.6

The same key-order defect dominated this evaluation. With sorted keys, the model was forced to emit an "insufficient" flag before writing any sentence, and produced empty answers for nearly every question; correctness measured 15 per cent. With declared key order and no other change, the same prompt scored 66.7 per cent.

Of the sixty questions after the fix: 34 were answered correctly, 11 were answered but wrong or incomplete, 6 were correctly declined as unanswerable, 8 were wrongly declined, and 1 was empty. Eight of the forty-nine sentences the model produced failed verification and were hidden from the user. Zero citations were fabricated.

Three qualifications apply to the 66.7 per cent figure. First, there is **no held-out split** for this evaluation: the second prompt version was written after seeing the first fail on these same sixty questions, so the figure may be optimistic. Second, retrieval ran without the embedding channel, since no embedding model was approved on the evaluation machine; the vector half of the hybrid search is therefore unmeasured end to end. Third, the facts used for retrieval came from a simulated oracle extraction rather than from the real extraction measured in Section 6.6, so this evaluation measures the answering pipeline rather than the complete system.

Latency was far outside target: a median of 25.2 seconds to the first sentence against a 2.5-second target, and 25.5 seconds to a complete answer against ten. Figure 6.3 shows this against the draft figures.

FIG:6.3

## 6.8|Draft Assistant

Table 6.7 gives the results.

TBL:6.7

The safety properties hold and the speed does not. Of twenty scenarios, seventeen produced valid drafts. **Zero unsupported claims were shown without a flag** — the independent fact checker caught every invented date, amount and name. In a real insertion test, a password field was detected and refused, a changed focus target was detected and refused, an unknown password state was treated as a password, and every clipboard format was restored at its original size with the exclusion formats present during the paste.

Three things could not be tested on the available machine. Clipboard history is disabled there, so the guarantee that the draft does not enter the history could not be exercised. Cloud clipboard upload cannot be observed without an account. And the five named applications in the criterion — two mail clients and three messaging applications — all require signed-in accounts; only a plain Windows text field was exercised.

## 6.9|Interface Performance and Capture Exclusion

Table 6.8 gives the interface measurements.

TBL:6.8

Capture exclusion was verified at the interface level by placing a magenta backdrop beneath the window and capturing the monitor twice, once through Windows Graphics Capture — the interface used by the Snipping Tool, Game Bar, Teams on Windows 11 and modern OBS capture — and once through the older GDI block-transfer path used by legacy recorders. The notch was wholly absent from both, in both its compact and expanded states. Two controls confirm the method: an unexcluded test window was fully visible in the same frames, and the same window with exclusion applied was fully hidden.

The criterion names five specific applications, none of which was run: two require signed-in accounts and a live screen share, and one was not installed. The result is therefore **partial**: the operating-system mechanism is verified, the applications are not.

Placement after a monitor change or a scaling change is likewise partial. The handlers are wired and the placement arithmetic is unit-tested for negative origins, monitors positioned above the primary, and three scaling factors; but only one monitor was available, so no live change was exercised.

Three defects were found by measuring rather than by testing, and are worth recording. A second graphics capture in the same process caused an access violation that would have crashed the application after the first use of the vision rung; the cause was initialising a new apartment-threaded context per capture, and the fix was a single persistent worker thread. The default hotkey was already registered by another application on the machine, so the toggle silently never worked; alternates were added and the active chord is now shown in the interface. And the input-to-visual latency was initially 292 milliseconds at the 95th percentile, nearly three times the target, before being reduced to 64.

## 6.10|Hardware Portability

Table 6.9 records which backends were exercised and how.

TBL:6.9

Two backend families have been exercised on real hardware: the processor fallback and the Vulkan path on an AMD integrated GPU. Five further families — CUDA, ROCm, SYCL, Metal and OpenCL — have had their **selection logic** verified against simulated adapter lists and simulated probe output in the exact format that real builds print, but no device of those families was available. This is stated precisely because the distinction matters: the tests prove that the arbitration code chooses correctly given a particular hardware report; they do not prove that any given vendor's driver works.

Several behaviours were nevertheless verified on real hardware and are meaningful: a software rendering adapter present on the machine was correctly ignored rather than selected; a build with an extra library planted in its directory was rejected by the manifest check; and the backend probe correctly ignored an inherited environment variable that would otherwise have restricted the visible devices.

## 6.11|Consolidated Scorecard

Table 6.10 brings every criterion together.

TBL:6.10

Of nineteen criteria, eleven are met — two of them with a longer-duration check still outstanding — seven are not met, and one has not been run for want of machine time. The division between them is not random. **Every criterion met is one enforced by deterministic code.** Every criterion missed depends either on the quality of a four-billion-parameter quantised model, on the throughput of an integrated GPU, or on a third-party runtime outside the application's control.

## 6.12|Threats to Validity

Several factors limit how far these results generalise, and they are stated here rather than left for a reader to infer.

**Hardware.** All model-dependent timing was measured on an integrated GPU with shared memory. The latency results say almost nothing about what a discrete GPU would achieve, and the project's own target hardware was never tested.

**Evaluation sets are synthetic.** The extraction, answering and draft sets are constructed items resembling emails, chats and documents, not captures of real activity, which would contain personal data. Real screen text is messier.

**No held-out split for answering.** As noted in Section 6.7, the second prompt was written after observing the first fail on the same questions.

**Application compatibility is thinly sampled.** Only seven of twenty target applications were tested for text acquisition; the rest were not installed or needed accounts. The gate asking for sixteen of twenty is unverified, not failed.

**Model non-determinism.** At least one injection case passed in one run and failed in another with identical settings, and five cases timed out in a run the machine interrupted. Single-run results on a generative model carry run-to-run variance that this evaluation did not quantify by repetition.

**Simulated results are not model results.** Where a number is labelled SIMULATED, a stand-in produced the model's output. Such a result validates the surrounding code and says nothing about the model.

## 6.13|Not Yet Evaluated

For completeness: the twenty-four-hour network and canary runs; the eight-hour resource and stability run; the seven-day stability run; the file-system watcher test confirming that no image file is ever written; password-field exclusion against native Windows Forms and WPF applications; capture exclusion inside the five named conferencing applications; draft insertion in the five named applications; clipboard-history and cloud-clipboard exclusion; placement across a live monitor or scaling change; the lock-and-unlock path that triggers the morning brief; end-to-end operation of the embedding channel; and inference on any backend other than Vulkan and the processor.
