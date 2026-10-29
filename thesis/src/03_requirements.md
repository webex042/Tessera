# CHAPTER 3|PROBLEM DEFINITION AND REQUIREMENTS ANALYSIS

## 3.1|Problem Statement

Build a Windows desktop application that maintains a searchable, question-answerable memory of what its single user has been working on, subject to four hard conditions:

+ Captured content and everything derived from it must remain on the user's machine, protected at rest, and must never be transmitted.
+ The language model that interprets the content must run on whatever graphics hardware the machine happens to have, or on the processor if it has none, without the application binary containing any vendor's libraries.
+ Every statement the system makes to the user must be traceable to a verbatim span of captured text, and any statement that cannot be so traced must not be shown.
+ Instructions embedded in captured text must not become instructions to the system.

The system must meet these conditions while remaining responsive enough to be used continuously in the background and while never taking an irreversible action on the user's behalf.

## 3.2|Stakeholders

The **primary user** is an individual working on a personal or work Windows machine, who wants recall of their own activity and is unwilling to send it to a service provider. They are assumed to be technically comfortable but not a developer, and to be unwilling to spend time configuring hardware.

**Third parties** appear in the captured text without having consented to anything: colleagues named in an email, correspondents in a chat. The system therefore stores only text that was already visible on the user's own screen, keeps a person's identity in a form that can be suppressed on request, and stores suppressed identities only as a salted digest so that the suppression list itself cannot be read back into names.

A **security reviewer** is treated as a stakeholder in their own right, which is why the redaction and injection corpora, the evaluation harness and the measured results are all part of the repository rather than private.

## 3.3|Functional Requirements

Table 3.1 lists the functional requirements.

TBL:3.1

Requirements FR-1 to FR-17 are implemented and are the subject of Chapters 4 to 6. FR-18 is specified but not implemented, and is discussed in Section 7.8 as future work. Throughout this thesis, a requirement marked *implemented* means that the code exists and is exercised by tests; it does not by itself mean that the associated quality target in Table 3.2 is met.

## 3.4|Non-Functional Requirements

The non-functional requirements were written as numeric gates before implementation began, so that the evaluation could not be adjusted to fit the outcome. Table 3.2 lists each requirement together with its target and the outcome measured in Chapter 6; it therefore serves both as a specification and as a summary of results.

TBL:3.2

Six of the twelve targets were met, four were missed, and two were not measured. The reasons are analysed in Chapter 6 and discussed in Chapter 7. Two general observations are worth making here. First, every target that depends only on deterministic code was met. Second, every target that depends on the quality of a four-billion-parameter quantised model, or on the throughput of an integrated GPU, was missed. That separation is the most informative result in this thesis.

## 3.5|Use Cases

The use-case diagram appears as Figure 3.1 in Section 1.6. Three cases are described in detail here because they drive the architecture.

**UC-1: Continuous capture.** The user works normally. On each change of foreground window or title, and otherwise at a configurable cadence, the system checks a set of gates, then attempts to read text from the active window, then redacts it, then merges it into an in-memory session. When attention moves elsewhere for thirty seconds, or after five minutes of continuous use, or when the accumulated text passes six thousand characters, or on session lock or shutdown, the session is written to the database as a single item. *Pre-condition:* capture is not paused — and capture begins paused on first run, so this is an action the user must take. *Alternative flow:* if any gate rejects the window, nothing is captured and a content-free audit event records the transition.

**UC-4: Ask a question with sources.** The user types a question. The system parses time, application and fact-kind filters from the question, runs the retrieval channels described in Section 4.9, selects up to eight snippets within a budget of roughly 1,500 tokens, and asks the model to answer using only those snippets, citing a snippet identifier for each sentence. As sentences stream back, each is checked against the snippet it cites. Verified sentences are shown with a citation chip that opens the underlying item; unverified sentences are hidden and counted. If nothing is verified, the system says it could not find the answer and offers the top three sources. *Post-condition:* the user is never shown an uncited claim.

**UC-7: Contextual draft.** With the cursor in a reply field, the user invokes the draft assistant. The system records the focused element's identity, checks that it is editable and is not a password field, reads the surrounding thread, retrieves relevant memory, and generates a draft at interactive priority. An independent checker then verifies every date, time, amount, number, email address, file name and personal name in the draft against the thread and the stored memory, flagging anything unsupported. The draft appears with its flags; nothing is inserted until the user presses Insert. On insertion the system re-verifies that the focus target is still the same element, saves every format of the current clipboard, writes the draft marked to be excluded from clipboard history and cloud synchronisation, sends the paste keystroke, and restores the clipboard. *The system never presses Enter.*

## 3.6|Constraints

**Platform.** Text acquisition, key protection, clipboard handling and window management depend on Win32, UI Automation and WinRT, so capture is Windows-only. The pipeline, storage, retrieval and guard libraries are deliberately platform-neutral and are compiled and tested without Windows, which is also how they are exercised in continuous integration.

**Hardware.** The reference machine specified in the plan was not available. All model-dependent measurement was performed on an AMD Ryzen 5 7530U with Radeon integrated graphics, which shares system memory with the processor and has far lower bandwidth than a discrete card. This constrains the validity of every latency figure in this thesis.

**No network.** Because the system may not make outbound connections, several conveniences are unavailable: no model can be fetched automatically, no dependency advisory database can be refreshed during a build, and no error reporting exists. Each of these had to be replaced with something offline — pinned digests, a cached advisory database, and a content-free diagnostics bundle that the user inspects before saving.

**Single developer, fixed schedule.** The project was carried out by one student within a single semester, which is the principal reason that the vision rung, the embedding channel and several real-hardware verifications are implemented but not fully exercised.

## 3.7|Hardware and Software Requirements

TBL:3.3

## 3.8|Assumptions

+ The user's Windows account is not already compromised. The data-protection key is bound to that account, so an attacker who controls it can decrypt the database. This is stated as a limitation in Section 7.5 rather than defended against.
+ The machine is physically secure while unlocked. Capture stops on session lock, but an unlocked machine with the interface open exposes whatever is on screen.
+ Captured text is in a language the model handles. The embedding model is multilingual; the extraction prompts and the injection classifier are written for English and, partially, for transliterated Indian-language text. Section 6.5 shows that multilingual attacks are among the defence's weakest categories.
+ The user supplies their own model files after an explicit approval step. No model is bundled.

## 3.9|Scope Boundaries and Non-Goals

The following are explicitly out of scope: universal hardware compatibility, since a GPU without any of the supported drivers falls back to the processor; autonomous action of any kind; cloud synchronisation, accounts or telemetry; multi-user operation; capture on operating systems other than Windows; and defence against an attacker who already controls the user's Windows account.

## 3.10|Requirement Traceability

Each functional requirement is realised by an identifiable part of the system and verified by an identifiable measurement. FR-1 and FR-2 are realised by the capture ladder and gate logic of Section 4.6 and verified in Section 6.3; FR-3 by the redaction engine of Section 5.5 and verified in Section 6.4.1; FR-4 and FR-6 by the storage schema of Section 4.8 and verified in Sections 6.4.2 and 6.6; FR-5 and FR-7 by the extraction pipeline and injection guard of Sections 5.9 and 5.10 and verified in Sections 6.5 and 6.6; FR-8 and FR-9 by the retrieval design of Section 4.9 and verified in Section 6.7; FR-10 and FR-11 by the rollup and open-loop logic of Section 5.13; FR-12 by the draft assistant of Section 5.12 and verified in Section 6.8; FR-13 and FR-14 by the process architecture and backend selection of Sections 4.5 and 4.7 and verified in Section 6.10; FR-15 by the gate logic; FR-16 by the network boundary of Section 5.15; and FR-17 by the diagnostics view.
