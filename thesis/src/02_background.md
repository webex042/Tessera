# CHAPTER 2|BACKGROUND AND LITERATURE REVIEW

## 2.1|Introduction

This chapter explains the concepts the rest of the thesis depends on, beginning with the most basic. Readers already familiar with quantised inference, retrieval and prompt injection may prefer to read only Section 2.8, which compares Tessera with related systems.

Each section states what the concept is, why Tessera needs it, and where the idea comes from. Published work is cited; where a statement describes this project's own implementation rather than published work, the text says so.

## 2.2|Local Artificial Intelligence Inference

### 2.2.1|What Inference Means Here

A large language model is, at its core, a very large table of numbers called *parameters* or *weights*, together with a fixed procedure for combining them. *Inference* is the act of running that procedure on an input to produce an output — in contrast to *training*, which is the much more expensive act of determining the weights in the first place. Tessera performs inference only; it never trains or fine-tunes a model.

The model used in this project, Qwen3-VL-4B-Instruct [2], is a *vision-language model*: it accepts images as well as text. The "4B" indicates roughly four billion parameters. Tessera uses the vision capability only as a last resort, when text cannot be obtained by other means.

### 2.2.2|Quantisation

Stored at full precision, four billion parameters at sixteen bits each would occupy about eight gigabytes — more than most consumer graphics cards have available. *Quantisation* reduces the number of bits used to represent each weight. The Q4_K_M format used here stores most weights at approximately four bits with a block-wise scaling factor, bringing the model file down to approximately 2.4 gigabytes. The trade-off is a loss of accuracy; how much loss is an empirical question, and the results in Chapter 6 are in part a measurement of it for this particular task.

The weights are distributed in the GGUF container format used by llama.cpp [1]. A GGUF file begins with a header describing the architecture, the number of layers and the quantisation type of each tensor. Tessera reads this header — and only the header, never the tensor data — to decide whether a given backend can execute the model and how much memory it will need.

### 2.2.3|Execution Backends

llama.cpp separates the model-independent logic from backend libraries that perform the actual arithmetic on a particular kind of hardware. A build for Vulkan contains a different acceleration library from a build for CUDA, although both contain the same server executable. This separation is what makes the architecture in Section 4.7 possible: Tessera can treat an entire backend as a replaceable, externally verified artefact rather than as a compile-time dependency.

## 2.3|Memory Systems and Information Retrieval

A memory system must be able to find the few items relevant to a question among many thousands. Two families of techniques are relevant, and Tessera uses both.

### 2.3.1|Lexical Retrieval

*Lexical* retrieval matches the words of the query against the words of the documents. The standard ranking function is BM25 [3], which scores a document higher when it contains the query terms often, when those terms are rare across the collection as a whole, and when the document is not unusually long. BM25 is implemented directly inside SQLite's FTS5 full-text extension [4], which Tessera uses, so the ranking happens inside the encrypted database rather than in application memory.

Lexical retrieval is precise when the user remembers the actual words, and useless when they do not.

### 2.3.2|Dense Retrieval

*Dense* retrieval converts both the query and each document into a vector of numbers — an *embedding* — chosen such that texts with similar meanings produce vectors that point in similar directions. Similarity is then measured by the cosine of the angle between them. Tessera uses the multilingual-e5-small model [5], executed through ONNX Runtime [25], which produces 384-dimensional vectors and, as its name indicates, handles multiple languages — relevant because screen text in Indian usage frequently mixes English with other languages.

Dense retrieval finds relevant material when the wording differs, and can be imprecise about names, numbers and rare terms.

### 2.3.3|Hybrid Retrieval and Rank Fusion

Because the two methods fail in different ways, combining them works better than either alone. The combination used here is Reciprocal Rank Fusion [6], which ignores the raw scores — they are not comparable between a BM25 score and a cosine similarity — and uses only the rank each method assigned. A document appearing at rank *r* in a list contributes 1/(k + r) to its total score, with k conventionally set to 60. A document ranked respectably by both methods therefore outranks one ranked first by a single method, which is the desired behaviour.

### 2.3.4|Near-Duplicate Detection

A memory system that samples the screen repeatedly will see the same text many times. Storing each copy would waste space and distort retrieval. SimHash [8] addresses this by producing a fixed-length fingerprint whose Hamming distance between two documents is small when the documents are similar. Manku and colleagues [9] established its practical use for near-duplicate detection at web scale. Tessera computes a 64-bit SimHash over overlapping groups of three consecutive words and treats a Hamming distance of three or less as "unchanged".

## 2.4|Grounding, Provenance and Hallucination

A language model asked to summarise text sometimes produces statements the text does not support. The survey by Ji and colleagues [11] distinguishes *intrinsic* hallucination, where the output contradicts the source, from *extrinsic* hallucination, where it adds material the source does not mention. Both are unacceptable in a memory system, because the user will later treat the record as what actually happened.

Retrieval-augmented generation [10] mitigates this by supplying the model with retrieved source passages and asking it to answer from them. This helps but does not guarantee anything, since the model may still depart from the passages it was given.

Tessera therefore adds a verification step that does not depend on the model's cooperation. The model is required to emit, alongside every proposed fact, a quotation it claims to have taken from the source. Code then checks that the quotation actually occurs there. The check proceeds in three stages — exact substring, substring after normalising whitespace and punctuation, and finally a fuzzy word-set match using the Dice coefficient [12] with a threshold of 0.9 — and a fact whose quotation fails all three is discarded rather than corrected. This is the mechanism behind the zero hallucination rate reported in Section 6.6.

## 2.5|Indirect Prompt Injection

Perez and Ribeiro [13] showed that user-supplied text can override an application's own instructions to a language model. Greshake and colleagues [7] extended this to the *indirect* case, in which the adversarial text is not supplied by the user at all but is retrieved by the application from a web page, an email or a document. The OWASP Top 10 for Large Language Model Applications [14] lists prompt injection as its first entry.

This is the central security problem for any screen-aware system, because an attacker who can place text on a page the user visits can address the model directly. Published defences fall into three broad groups: instructing the model to disregard embedded instructions, which a sufficiently persuasive injection defeats; filtering or classifying input before it reaches the model; and constraining what the system is permitted to do with the output, so that a successful injection has no useful effect.

Tessera relies mainly on the third and partly on the second. The model is given no tools: it cannot send, delete, fetch or execute anything, and its only output channel is a JSON object constrained by a grammar. A successful injection can therefore at most cause a misleading entry to be stored as text. Section 5.10 describes the classifier that attempts to prevent even that, and Section 6.5 measures how often it succeeds against attacks it has never seen.

## 2.6|Privacy-Preserving Desktop Software

### 2.6.1|Encryption at Rest

Tessera stores everything in SQLCipher [15], an extension of SQLite that encrypts the entire database file — including the schema, the indexes and the write-ahead log — with AES-256. Encryption only moves the problem to the key, which must be available to the application but not to anyone else. On Windows the standard answer is the Data Protection API, which encrypts a blob such that only the same user account on the same machine can decrypt it. Tessera stores the wrapped key in a file [24], unwraps it at start-up, passes the raw key to SQLCipher and then overwrites the buffer that held it.

### 2.6.2|Redaction and Checksum Validation

Detecting secrets in free text by pattern alone produces an unacceptable number of false positives: a twelve-digit number is not necessarily an Aadhaar number, and a sixteen-digit number is not necessarily a payment card. Both identifier schemes, however, carry check digits. Payment card numbers satisfy the Luhn algorithm, standardised in ISO/IEC 7812-1 [17]. Aadhaar numbers satisfy the Verhoeff algorithm [18], [27], a decimal check-digit scheme based on the dihedral group of order ten which, unlike Luhn, detects all single-digit and all adjacent-transposition errors. Requiring the checksum to agree before redacting is what allows Tessera to redact every seeded secret while flagging fewer than one clean line in two hundred.

## 2.7|Desktop Application Architecture on Windows

### 2.7.1|Obtaining Text From Another Application

Windows offers no single way to read what another program is displaying. Three mechanisms are relevant, and Tessera uses all three in sequence.

**UI Automation** [20] is the accessibility framework that screen readers use. It exposes a tree of elements with their text, and marks password fields with an `IsPassword` property so that assistive technology does not read them aloud — a property Tessera relies on to exclude them. Its drawback is that calls are synchronous requests into the target application and can block indefinitely if that application is busy; the project measured a 2.8-second block against Microsoft Word.

**Windows Graphics Capture** [21] produces a frame of a window or monitor as an in-memory bitmap. Combined with the WinRT optical-character-recognition engine, it yields text from applications whose accessibility tree is unhelpful. Because the frame is a bitmap, it carries no structure, so password fields must be blanked by position before recognition runs.

**A vision-language model** can read a rendered image directly and is used only when recognition quality is poor.

### 2.7.2|Process Isolation

Windows Job Objects [22] group processes so that limits apply to the group as a whole. The flag `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` instructs the operating system to terminate every process in the job when the last handle to the job closes. Since the handle closes when the parent process exits for any reason, including a crash, this is a reliable guarantee that no orphaned inference worker is left running. Creating the child suspended, assigning it to the job, and only then resuming it closes the window in which a child could escape.

The application shell itself is built with Tauri 2 [19], which hosts a web interface in the system WebView2 runtime [28] and exposes typed commands to it, rather than bundling a complete browser engine.

## 2.8|Related Systems and How Tessera Differs

Table 2.1 sets out how Tessera compares with the nearest alternatives on the four properties this thesis is about.

TBL:2.1

**Windows Recall.** Microsoft's Recall feature, available on Copilot+ PCs, periodically saves snapshots of the screen and makes them searchable. After an initial announcement drew substantial criticism, Microsoft made the feature opt-in, encrypted the snapshot store and gated access behind Windows Hello authentication [16]. Recall is the closest commercial relative of this work, and the comparison is instructive rather than competitive: Recall is a first-party operating-system feature with access to hardware security that an ordinary application does not have, and it requires a qualifying neural processing unit. Tessera differs in three respects that are visible to the user. It stores extracted text and structured facts rather than images, so there is no snapshot to exfiltrate. It attaches a verbatim source span to every fact and verifies each answer sentence against it, so an answer can be checked. And it publishes the measured results of an adversarial corpus against its own injection defences, which Section 6.5 presents including the failures.

**General-purpose local model runners.** Tools that run a llama.cpp server behind a chat interface solve the inference problem and none of the others. They have no capture pipeline, no redaction, no provenance and no injection defence, because they are not memory systems. Tessera's inference layer is comparable to theirs; the remaining ninety per cent of the system is what this thesis is about.

**Research on retrieval-augmented generation.** The retrieval architecture here is conventional and makes no claim to novelty: BM25 and dense retrieval fused by Reciprocal Rank Fusion is an established recipe. What is specific to this work is applying per-sentence citation verification as a hard gate — an unverified sentence is hidden from the user rather than shown with a caveat.

## 2.9|Summary

The techniques Tessera uses are, individually, well established: quantised local inference, hybrid retrieval with rank fusion, SimHash deduplication, checksum-validated redaction, encrypted local storage and operating-system process isolation. What this project contributes is their arrangement under a single discipline — that no guarantee may depend on the language model behaving correctly — and a measurement of how much of that discipline survives when a four-billion-parameter model meets a real adversarial corpus on commodity hardware. The next chapter turns the problem statement into requirements that can be checked.
