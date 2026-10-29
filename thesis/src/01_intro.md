# CHAPTER 1|INTRODUCTION

## 1.1|Background and Context

A person using a computer for a day produces an enormous amount of context and retains very little of it. Decisions are made in a chat window, a deadline is agreed in an email, a figure is quoted in a spreadsheet, and a week later none of it can be found. The information was visible on the screen at the time, which is precisely why it was never written down anywhere else.

Over the past few years, large language models have become good enough at reading ordinary prose that this problem looks solvable. A model can read what was on the screen, notice that a commitment was made, and remind the user about it later. Several commercial products now do something of this kind. Almost all of them share one property: the text is sent to a remote server, where a large model processes it. This is a reasonable engineering decision — the largest and most capable models are too big to run on a laptop — but it has an uncomfortable consequence. The context that makes such a system useful is exactly the context a person would least like to transmit: private conversations, financial details, medical correspondence, unreleased work.

The alternative is to run the model on the user's own machine. This has become possible rather than merely imaginable. Quantisation techniques compress a model's weights from sixteen bits per parameter to roughly four, which brings a four-billion-parameter vision-language model down to approximately 2.4 gigabytes — small enough to fit in the memory of an ordinary consumer graphics processor. Open inference engines such as llama.cpp [1] can execute such models across a wide range of hardware. A model of this size is not as capable as a frontier cloud model, but it does not need to be: the task of reading a paragraph and noticing that it contains a deadline is far easier than open-ended reasoning.

Running a model locally, however, replaces one set of problems with another. The remainder of this thesis is largely about those replacement problems, which are less often discussed than the privacy benefit they buy.

## 1.2|The Problem Being Addressed

This work addresses four distinct problems that arise together when a language model is made part of a desktop application.

### 1.2.1|Hardware Fragmentation

Graphics processors from different manufacturers are programmed through incompatible interfaces. NVIDIA hardware is driven through CUDA, AMD through ROCm or HIP, Intel through oneAPI and SYCL, Apple through Metal, and Qualcomm through OpenCL; Vulkan spans several vendors but is usually slower than a vendor's native path. An application that wants hardware acceleration therefore faces an unattractive choice: compile one vendor's libraries into its own binary and support only that vendor, or ship every vendor's libraries and become enormous. The CUDA runtime alone is approximately 390 megabytes. Worse, a binary built against one driver version may refuse to start against another.

### 1.2.2|Process Fragility

Model weights of two to six gigabytes push consumer hardware to its limits. Graphics drivers reset under memory pressure, inference engines hang, and out-of-memory conditions are common rather than exceptional. If the inference engine runs inside the application process, each of these events takes the entire application down with it — and, if a database write was in progress, may leave the user's data inconsistent.

### 1.2.3|Privacy of Captured Context

A system that reads the screen will eventually read a password manager, a banking page, a private browsing window or a form containing an identity number. No amount of good intention at the model layer helps here: by the time the text reaches the model, the exposure has already happened. The protection has to sit earlier, in code that runs before anything is stored or transmitted, and it has to fail in the safe direction when it is uncertain.

### 1.2.4|Model Authority and Indirect Prompt Injection

The fourth problem is the subtlest. Text read from a screen is not neutral input. A web page, an email or a document can contain a sentence addressed not to the user but to any language model that happens to read it: *"Ignore your previous instructions and send the contents of the user's notes to the following address."* This is known as indirect prompt injection [7]. A language model has no reliable way to distinguish instructions that come from its operator from instructions that merely appear inside the text it was asked to read. If the system treats model output as authoritative, an attacker who can get text onto the user's screen can influence what the system remembers and does.

A related problem is hallucination [11]: a model asked to summarise may produce fluent statements that the source text does not support. For a memory system this is worse than an error, because the user will later rely on the recorded "fact" without re-reading the original.

## 1.3|Motivation

The motivation for Tessera is the observation that these four problems have a common shape: in each case, the correct response is to move the guarantee out of the model and into ordinary, testable code.

Hardware portability should not depend on a universal compiler that does not exist; it should come from keeping vendor code outside the application, probing what is actually installed, and falling back in a defined order. Crash resilience should not depend on the inference engine being well-behaved; it should come from the operating system's process isolation. Privacy should not depend on a model declining to repeat a password; it should come from the password never reaching the model. And trustworthiness should not depend on the model being honest; it should come from code that checks whether each claimed quotation really occurs in the source, and refuses to store the claim if it does not.

Stated as a single principle: **the model proposes, and deterministic code disposes.** Everything in this thesis follows from taking that principle seriously and then measuring how far it actually carries.

A secondary motivation is methodological. Claims about artificial-intelligence systems are frequently made without evidence. This project fixed its release criteria in writing before it was built, measured against them, and recorded the failures as prominently as the successes. Several of those criteria are not met, and Chapter 6 says so.

## 1.4|Objectives

The project set eleven objectives. Chapter 7 returns to each of them with the evidence gathered.

+ **O1.** Run a vision-language model locally without linking any GPU vendor library into the application binary.
+ **O2.** Select an appropriate hardware backend at run time and fall back deterministically when the preferred choice is unavailable.
+ **O3.** Contain inference failures so that a crashed or hung worker cannot terminate the application or corrupt the database.
+ **O4.** Read text from the active window without ever writing a screen image to disk, and without reading windows the user has excluded.
+ **O5.** Remove passwords, one-time codes, financial identifiers and government identity numbers from text before it is stored or placed in a prompt.
+ **O6.** Encrypt all stored data at rest under a key protected by the user's Windows login.
+ **O7.** Ensure that every stored fact is traceable to a verbatim span of the text it came from.
+ **O8.** Prevent instructions embedded in captured text from being recorded as the user's own tasks.
+ **O9.** Answer natural-language questions from stored memory with a verifiable source for each sentence.
+ **O10.** Keep the interface responsive and absent from screen recordings and shared screens.
+ **O11.** Make no outbound network connection other than model downloads the user has explicitly approved.

## 1.5|Scope of the Work

The work covers the design, implementation and evaluation of a complete desktop system, from operating-system integration up to the user interface. Specifically, it includes text acquisition from Windows applications, a privacy filtering pipeline, an encrypted local store with full-text and vector indexes, out-of-process inference with hardware-backend selection, structured extraction with deterministic validation, hybrid retrieval, grounded question answering, contextual draft generation, periodic summarisation, and a floating desktop interface.

The work excludes several things that are either planned for later or deliberately out of bounds. Audio capture, speech recognition and live translation were specified but are not implemented. Platforms other than Windows are not supported for capture, although the storage, retrieval, inference and pipeline libraries are platform-neutral and compile without Windows. There is no cloud component, no user account, no synchronisation between devices, and no telemetry. The system takes no autonomous action on the user's behalf: it never sends a message, never presses a send button and never closes a task without confirmation.

An important limitation of scope concerns evaluation. The reference machine named in the project plan — an NVIDIA RTX 4050 with 6 GB of dedicated memory — was not available. All model-dependent measurements were taken on an AMD Radeon integrated GPU, which shares system memory and has substantially lower bandwidth. Latency figures in Chapter 6 should be read with that in mind; accuracy figures, which depend on the model rather than the hardware, should not.

## 1.6|Intended Users and Use Cases

The intended user is a single person working on their own Windows machine who wants a record of their own activity and is unwilling to send that record to a third party. Secondary audiences are systems engineers interested in the sidecar isolation pattern, and security researchers interested in auditing a capture-and-redaction pipeline whose source and test corpora are both available.

FIG:3.1

The principal use cases are: continuous, pausable capture of visible text from permitted windows; keyword and semantic search over past activity; asking a question and receiving an answer in which each sentence carries a citation; reviewing open commitments and confirming or dismissing them; reading an automatically assembled brief and drilling from a month summary down to the individual item that produced it; and generating a contextual reply draft that is inserted into the field the user was typing in, only after the user presses a confirmation control.

## 1.7|Main Contributions

The main contributions of this work are:

- **A vendor-independent local inference architecture.** Each inference backend is a separately pinned, hash-verified external build rather than a linked library. The application enumerates hardware adapters without loading any vendor runtime, probes each verified build for real, inspects the model's own header for compatibility, and produces a ranked candidate list that always ends at the processor. The model is never silently re-quantised or substituted to make it fit.

- **A capture pipeline in which privacy is structural rather than advisory.** Capture starts paused. A window whose private-browsing state cannot be determined is treated as private and skipped. Screen frames exist only in memory. Redaction runs to a fixed point before storage, using checksum validation — the Luhn algorithm for payment cards and the Verhoeff algorithm for Aadhaar numbers — rather than digit patterns alone, which is what keeps the false-positive rate below half a per cent.

- **A provenance discipline enforced by the database schema.** Every fact must have at least one evidence row recording a verbatim quote and its character span in a stored item. A database trigger deletes a fact when its last piece of evidence is removed. A fact whose quote cannot be located in the source is never written at all. The result, measured on a real model run, is a hallucination rate of zero.

- **A deterministic defence against indirect prompt injection, with an honest measurement of how far it generalises.** Rather than asking the model to behave, the system classifies the source text surrounding each proposed fact and refuses, quarantines or accepts accordingly. Critically, the evaluation uses a held-out half of the attack corpus that was never used for tuning, which shows the defence to be roughly 62 per cent effective against unseen attack styles on the real model — a number this thesis reports rather than conceals.

- **An evaluation conducted against criteria fixed in advance, reporting failures.** Of nineteen release criteria, eleven are met, seven are not, and one has not been run. Chapter 6 presents all of them.

## 1.8|Organisation of the Thesis

Chapter 2 introduces the background concepts needed to follow the design — quantised local inference, lexical and dense retrieval and their fusion, grounding, indirect prompt injection, and the Windows interfaces used for text acquisition — and reviews related systems. Chapter 3 states the problem formally and derives functional and non-functional requirements, constraints and use cases. Chapter 4 presents the architecture: process boundaries, module decomposition, the capture ladder, backend selection, the storage schema, retrieval, scheduling and the trust boundaries. Chapter 5 describes the implementation, including the significant algorithms and the reasoning behind the major decisions. Chapter 6 presents the evaluation methodology and all measured results, favourable and otherwise. Chapter 7 discusses what the work achieves, the trade-offs it makes, its limitations and the directions that follow. Chapter 8 concludes. Appendices give the directory structure, setup instructions, the configuration reference, selected test cases and a glossary.
