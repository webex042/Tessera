# Table data for the Tessera thesis.
# Each entry: id -> dict(caption=..., cols=[widths in inches or None], rows=[[...]])
# Row 0 is always the header row.

TABLES = {

"2.1": dict(
caption="Comparison of related approaches to personal screen memory and local inference",
rows=[
 ["Approach", "Where data is processed", "Evidence linked to answers", "Defence against injected screen text", "Hardware portability"],
 ["Cloud assistant with screen sharing", "Remote servers", "Varies; usually not shown", "Provider-dependent, not inspectable by the user", "Not applicable (server side)"],
 ["Windows Recall (Copilot+ PC)", "On device; encrypted snapshot store, Windows Hello gated [16]", "Returns snapshots, not sentence-level citations", "Not documented as a published, measured guarantee", "Requires a qualifying NPU-equipped device"],
 ["General local LLM runners (e.g. a llama.cpp server with a chat front end)", "On device", "None by default", "None by default", "Good; depends on the build the user installs"],
 ["Tessera (this work)", "On device, loopback only; one auditable network crate", "Every stored fact carries a verbatim source span; answers verified sentence by sentence", "Deterministic classifier on the source text; measured on a 152-case corpus", "Runtime probe and ranked fallback over seven backend families; two verified on real hardware"],
]),

"3.1": dict(
caption="Functional requirements",
rows=[
 ["ID", "Requirement", "Status in this work"],
 ["FR-1", "Detect the foreground window and read its visible text without the user copying or pasting anything.", "Implemented"],
 ["FR-2", "Refuse to read blocklisted applications, blocklisted sites and private or incognito browser windows.", "Implemented"],
 ["FR-3", "Remove secrets and personal identifiers from text before it is stored or sent to a model.", "Implemented"],
 ["FR-4", "Store captured text, and everything derived from it, in an encrypted local database.", "Implemented"],
 ["FR-5", "Turn stored text into structured facts (task, commitment, decision, event, question, note), each tied to a verbatim source quote.", "Implemented"],
 ["FR-6", "Reject any model-produced fact whose quote does not occur in the source text.", "Implemented"],
 ["FR-7", "Prevent instructions found inside captured text from being stored as the user's own tasks.", "Implemented (partially effective; see Chapter 6)"],
 ["FR-8", "Search past activity by keyword and by meaning, and combine the two rankings.", "Implemented (vector channel not yet exercised end to end)"],
 ["FR-9", "Answer a natural-language question from stored memory, showing a source for each sentence.", "Implemented"],
 ["FR-10", "Produce hour, day, week and month summaries and a morning brief, each citing the items it came from.", "Implemented"],
 ["FR-11", "List open loops and propose completion only when evidence supports it, never closing anything automatically.", "Implemented"],
 ["FR-12", "Draft a contextual reply and insert it into the field the user was typing in, after explicit confirmation.", "Implemented"],
 ["FR-13", "Run model inference in a separate operating-system process that cannot take the application down with it.", "Implemented"],
 ["FR-14", "Select a GPU backend automatically at run time and fall back in a defined order, ending at the CPU.", "Implemented"],
 ["FR-15", "Let the user pause all capture instantly and start the application in the paused state.", "Implemented"],
 ["FR-16", "Download model files only after an explicit user approval, and only when the file matches a pinned digest.", "Implemented"],
 ["FR-17", "Expose a diagnostics view containing counts and states but no captured content.", "Implemented"],
 ["FR-18", "Capture meeting audio and produce live translation.", "Not implemented (planned, Stage 8)"],
]),

"3.2": dict(
caption="Non-functional requirements and their measured outcome",
rows=[
 ["ID", "Requirement", "Target", "Measured outcome"],
 ["NFR-1", "No outbound network connection from the application except approved model downloads.", "Zero connections over 24 h", "Application process: zero in a 10-minute monitored run. The WebView2 runtime made one HTTPS connection (Section 6.4.3). Not met as written."],
 ["NFR-2", "No captured text readable on disk without the user's Windows login.", "Zero plaintext hits", "Zero hits across 285 files / 32 MB after a 60-second run; positive control detected."],
 ["NFR-3", "No screen image ever written to disk.", "Zero image files", "No image-writing code path exists; the file-system watcher test is still outstanding."],
 ["NFR-4", "A text read must never block the capture loop.", "≤ 300 ms wall clock", "Maximum 294 ms over 16 windows and 9 applications."],
 ["NFR-5", "Secrets removed before storage.", "100 % of corpus; ≤ 2 % false positives", "355 / 355 removed; 1 / 242 clean lines flagged (0.41 %)."],
 ["NFR-6", "Interface responsiveness.", "60 fps; input-to-visual ≤ 100 ms", "Frame p95 16.8 ms; input-to-visual p95 64 ms."],
 ["NFR-7", "Interactive answer latency.", "First token ≤ 2.5 s, full answer ≤ 10 s", "25.2 s / 25.5 s on the available integrated GPU. Not met."],
 ["NFR-8", "Draft latency.", "First text ≤ 3 s, complete ≤ 8 s", "5.9 s / 16.4 s on the available integrated GPU. Not met."],
 ["NFR-9", "Extraction quality.", "F1 ≥ 0.70; hallucination ≤ 3 %", "F1 0.533; hallucination 0.0 %. Partially met."],
 ["NFR-10", "The interface must not appear in screen recordings or shared screens.", "Absent from captures", "100 % absent from Windows Graphics Capture and GDI frames; named conferencing applications not yet exercised."],
 ["NFR-11", "Background resource use.", "CPU ≤ 3 %, RAM ≤ 250 MB over 8 h", "Not measured; the 8-hour run has not been performed."],
 ["NFR-12", "The application must build without any GPU vendor SDK installed.", "Clean build", "Verified; continuous integration builds the CPU-only path."],
]),

"3.3": dict(
caption="Hardware and software requirements",
rows=[
 ["Category", "Requirement"],
 ["Operating system", "Windows 11, 64-bit (the capture, clipboard and key-protection code uses Win32, UI Automation and WinRT interfaces)."],
 ["Processor", "x86-64 with AVX2; used for the embedding model and as the inference fallback."],
 ["Memory", "8 GB minimum; 16 GB recommended when the vision-language model is resident."],
 ["Graphics (optional)", "Any GPU with a CUDA, ROCm, Vulkan 1.2+, SYCL, Metal or OpenCL driver. Without one, inference runs on the CPU."],
 ["Disk", "Approximately 4 GB for the quantised model and projection file, plus the growing encrypted database."],
 ["Rust toolchain", "Stable MSVC toolchain, edition 2024 (rustc 1.99 or newer)."],
 ["C/C++ toolchain", "Visual Studio Build Tools with the C++ workload, required to compile the vendored OpenSSL used by SQLCipher."],
 ["Perl", "Strawberry Perl on PATH, required by the OpenSSL build."],
 ["Node.js and pnpm", "Node.js 24 or newer with pnpm 12.15.0, for the interface bundle."],
 ["Python", "Python 3.11 or newer, for the evaluation harness and the dependency checker."],
 ["Inference runtime", "A pinned llama.cpp build (tag b11900) fetched by the user for the chosen backend."],
 ["Models", "Qwen3-VL-4B-Instruct Q4_K_M with its F16 projection file, and multilingual-e5-small in ONNX form, each fetched only after explicit approval."],
]),

"4.1": dict(
caption="Workspace crates and their responsibilities",
rows=[
 ["Crate", "Responsibility", "Rust source (lines)"],
 ["tsr-core", "Domain primitives, the Sensitive<T> wrapper that prints as <redacted>, and the allowlist-based content-free logging layer.", "475"],
 ["tsr-win", "Safe wrappers over Win32 and WinRT: DPAPI, Job Objects, display affinity, window event hooks, clipboard, TCP table queries.", "1,771"],
 ["tsr-capture", "The three-rung text acquisition ladder, the deadline worker, change detection and session aggregation.", "5,101"],
 ["tsr-guard", "Blocklist evaluation, the redaction engine and the claim checker. Platform-neutral, so it compiles and tests without Windows.", "1,651"],
 ["tsr-store", "SQLCipher database, forward-only migrations, full-text indexes and the repository layer.", "3,081"],
 ["tsr-retrieval", "Pinned ONNX Runtime loading, embedding, chunking, the hybrid search engine and the Ask pipeline.", "1,621"],
 ["tsr-infer", "Adapter discovery, GGUF header parsing, backend selection, the sidecar supervisor, the loopback HTTP client and the priority scheduler.", "8,664"],
 ["tsr-net", "The only network-capable crate: approval-gated, resumable, digest-verified model downloads.", "1,115"],
 ["tsr-pipeline", "Extraction, schema parsing, evidence verification, the injection guard, date and people resolvers, rollups, briefs and open loops.", "7,254"],
 ["tsr-draft", "Draft generation, the independent fact checker and the draft state machine.", "587"],
 ["src-tauri", "The desktop shell: window placement, tray, hotkeys, typed IPC commands and the view layer.", "3,138"],
 ["evals/runner", "The tsr-eval harness that runs the extraction, Ask, draft, injection and backlog evaluations.", "2,331"],
 ["bench/uia-probe", "The standalone probe used to measure text acquisition against real applications.", "407"],
]),

"4.2": dict(
caption="Inference backends, the hardware they serve, and their verification status in this work",
rows=[
 ["Backend", "Hardware served", "Automatic order for that vendor", "Verification status"],
 ["CPU", "Any x86-64 with AVX2, or ARM64", "Always the final fallback", "Verified on real hardware"],
 ["Vulkan", "AMD, Intel, NVIDIA, Qualcomm", "Second choice for every GPU vendor", "Verified on real hardware (AMD Radeon integrated GPU)"],
 ["CUDA", "NVIDIA", "First choice on NVIDIA", "Selection logic verified against simulated adapters only"],
 ["ROCm (HIP)", "AMD discrete, RDNA2 and newer", "First choice on AMD", "Selection logic verified against simulated adapters only"],
 ["SYCL", "Intel Arc and Xe", "First choice on Intel", "Selection logic verified against simulated adapters only"],
 ["Metal", "Apple Silicon", "Only choice on Apple", "Selection logic verified against simulated adapters only"],
 ["OpenCL", "Qualcomm Adreno", "First choice on Qualcomm", "Selection logic verified against simulated adapters only"],
 ["OpenVINO", "Intel GPU and NPU", "Never chosen automatically; used only when forced", "Selection logic verified against simulated adapters only"],
]),

"4.4": dict(
caption="Trust boundaries in the system",
rows=[
 ["Boundary", "What crosses it", "Control applied"],
 ["Interface process to host process", "Typed command names and identifiers", "Fixed command set, validated lengths, enumerations and identifiers; errors carry a code, never content."],
 ["Host process to inference worker", "Prompt text and generated tokens", "Loopback address only; the listening socket's owning process identifier must equal the spawned child's."],
 ["Application to database", "Captured text, facts, summaries", "Whole-database encryption; the key is unwrapped from the Windows login and wiped from memory after use."],
 ["Application to the internet", "Model file bytes", "One crate holds all network code; it requires a typed approval value and verifies a pinned digest."],
 ["Captured screen text to the model prompt", "Untrusted text", "Escaped and wrapped in delimiters; the model has no tools; its output is constrained by a grammar and re-validated in code."],
 ["Model output to storage", "Proposed facts", "Schema check, evidence must exist in the source, injection classification, second redaction pass."],
]),

"4.3": dict(
caption="Database tables created by the four forward-only migrations",
rows=[
 ["Migration", "Tables and indexes created", "Purpose"],
 ["0001", "blocklist, settings, audit_log", "Capture policy, application settings and the content-free audit trail."],
 ["0002", "sources, items, items_fts (+ three synchronisation triggers)", "What was read, from where and when, with the full-text index kept in step by triggers."],
 ["0003", "people, person_aliases, person_merge_suggestions, suppressed_aliases, facts, fact_evidence, facts_fts, jobs, chunks", "Identity resolution, structured facts with their evidence spans, durable job state and embedding vectors."],
 ["0004", "summaries, summary_sources, briefs", "Hour, day, week and month rollups with provenance, and the assembled morning brief."],
]),

"5.1": dict(
caption="Architecture decision records and the questions they settle",
rows=[
 ["Record", "Question settled", "Outcome"],
 ["ADR-0001", "Which dependency versions and feature flags carry privacy guarantees?", "Exact version pins; default features of the embedding crates disabled so that no HTTP client or build-time download enters the graph; the ONNX Runtime API level raised so that the system copy cannot be used."],
 ["ADR-0002", "What to do about an outbound connection made by the WebView2 runtime?", "Open. Recorded as a failure of the zero-network criterion rather than explained away."],
 ["ADR-0003", "Should the automatic updater ship?", "No. It is compiled out because a transitive compression library carries unpatched advisories."],
 ["ADR-0004", "How is GPU-vendor independence achieved?", "Backends live outside the binary as pinned, hash-verified builds; the application detects, probes, ranks and falls back; the model is never silently changed to fit."],
 ["ADR-0005", "What redaction thresholds are acceptable?", "100 % recall on the seeded corpus with a false-positive ceiling of 2 % on clean text."],
 ["ADR-0006", "How is the understanding pipeline staged and validated?", "Durable, preemptible jobs; strict schema; evidence as source spans; a second redaction pass; a single atomic write."],
 ["ADR-0007", "How is the floating interface placed and hidden?", "One fixed-size transparent window with per-panel regions; display affinity set on every window and read back."],
 ["ADR-0008", "How are rollups, drafts and Ask guarded against a compromised model?", "Independent code-side checks: provenance validation, an independent claim checker, per-sentence citation verification."],
 ["ADR-0009", "What did the security audit change?", "Draft flagging for injected thread lines, a fallible date resolver, redaction run to a fixed point, model digest checking at start-up."],
 ["ADR-0010", "Why were answers empty?", "JSON schema keys were being serialised in alphabetical order, so the grammar forced the model to commit to an \"insufficient\" field before writing any answer. Declared key order was restored."],
]),

"6.1": dict(
caption="Evaluation environment",
rows=[
 ["Item", "Value"],
 ["Operating system", "Windows 11, build 26300"],
 ["Processor", "AMD Ryzen 5 7530U, 12 logical cores, AVX2 / FMA / F16C"],
 ["System memory", "15.3 GiB"],
 ["Graphics", "AMD Radeon integrated GPU, used through the Vulkan backend"],
 ["Display", "One 1920 × 1080 monitor at 150 % scaling"],
 ["Inference runtime", "llama.cpp build b11900, Vulkan variant, run as an external process"],
 ["Language model", "Qwen3-VL-4B-Instruct, Q4_K_M quantisation, with the F16 projection file"],
 ["Embedding model", "multilingual-e5-small (ONNX); approved for code paths but not run end to end"],
 ["Reference machine in the plan", "NVIDIA RTX 4050, 6 GB — not available; every timing below is from the integrated GPU"],
]),

"6.2": dict(
caption="Automated test and evaluation inventory",
rows=[
 ["Category", "Count", "What it covers"],
 ["Rust test functions", "375", "Unit and integration tests across the ten crates and the desktop shell; 17 are marked as requiring real hardware or a real desktop session."],
 ["Secrets corpus", "355 seeded cases + 242 clean lines", "Aadhaar including virtual identifiers, PAN, passport, voter identity, bank accounts, IFSC, UPI, cards, API keys, tokens and private keys."],
 ["Injection corpus", "152 cases (147 adversarial, 5 genuine controls)", "Thirty-eight attack categories including hidden text, fake system prompts, schema override, store poisoning, multilingual and OCR-derived attacks."],
 ["Extraction evaluation set", "80 items / 126 gold facts, split into development and held-out halves", "Chat, email, work and mixed screen content with verbatim gold evidence."],
 ["Question answering set", "60 questions", "Past-oriented questions over a seeded store."],
 ["Draft scenarios", "20 scenarios", "Each with must-include and must-not-include expectations."],
 ["Randomised robustness tests", "20,000 inputs per target; 3,000 random chunkings per streaming parser", "Redaction, date resolution, incremental JSON parsing and text normalisation."],
]),

"6.10": dict(
caption="Consolidated scorecard against the project's own release criteria",
rows=[
 ["Criterion", "Target", "Measured", "Verdict"],
 ["Secrets removed from the corpus", "100 %", "355 / 355", "Met"],
 ["Redaction false positives", "≤ 2 %", "0.41 %", "Met"],
 ["Plaintext on disk (canary scan)", "0 hits", "0 hits in 285 files / 32 MB", "Met for the 60-second run; the 24-hour run is outstanding"],
 ["Hallucinated stored facts", "≤ 3 %", "0 / 97 = 0.0 %", "Met"],
 ["Schema-invalid stored rows", "0", "0", "Met"],
 ["Fabricated citations in answers", "0", "0", "Met"],
 ["Facts never closed without confirmation", "0 automatic closures", "0; enforced by a source scan", "Met"],
 ["Insertion into a password or changed field", "Never", "Refused in a real test", "Met"],
 ["Interface frame rate", "60 fps", "p95 frame 16.8 ms", "Met"],
 ["Input-to-visual latency", "≤ 100 ms", "p95 64 ms", "Met"],
 ["Interface absent from screen captures", "Absent", "100 % absent from Graphics Capture and GDI frames", "Met at the interface level; named applications outstanding"],
 ["Extraction F1 (held-out)", "≥ 0.70", "0.533", "Not met"],
 ["Answers correct", "≥ 80 %", "66.7 %", "Not met"],
 ["Answer sentences verified", "≥ 90 %", "83.7 %", "Not met"],
 ["Time to first answer sentence", "≤ 2.5 s", "25.2 s", "Not met on the available hardware"],
 ["Complete draft", "≤ 8 s", "16.4 s", "Not met on the available hardware"],
 ["Zero outbound connections", "0", "0 from the application; 1 from the WebView2 runtime", "Not met as written"],
 ["Injection corpus", "No injected instruction stored as a user task", "123 / 152 overall; 24 / 39 on the untouched half", "Not met"],
 ["Resource use over 8 h and 7-day stability", "CPU ≤ 3 %, RAM ≤ 250 MB; no crash", "Not run", "Outstanding"],
]),

"7.1": dict(
caption="Project objectives and the evidence for each",
rows=[
 ["Objective", "Outcome", "Evidence"],
 ["O1: Run a vision-language model locally without linking any GPU vendor library into the application.", "Achieved", "No vendor library appears in the dependency graph; the application builds and runs on a machine with no GPU SDK; backends are separate hash-verified builds."],
 ["O2: Select a backend at run time and fall back deterministically.", "Achieved in code; partially verified on hardware.", "Ranked candidate selection with CPU last; two backend families exercised on real hardware, five on simulated adapters."],
 ["O3: Contain inference failures so they cannot take the application down.", "Achieved", "Suspended creation, cleared environment, Job Object with kill-on-close, port-owner authentication, supervisor with backoff and restart budget."],
 ["O4: Read screen text without writing images to disk and without reading blocked windows.", "Achieved", "Capture ladder with an in-memory frame path; blocklist defaults to blocking when a private state cannot be read."],
 ["O5: Remove secrets before anything is stored.", "Achieved", "355 of 355 seeded secrets removed at a 0.41 % false-positive rate."],
 ["O6: Encrypt everything at rest under the user's Windows login.", "Achieved", "Whole-database encryption; a wrong key is rejected and writes nothing to standard error; no plaintext found on disk."],
 ["O7: Ensure every stored fact is traceable to a verbatim source span.", "Achieved", "No fact can exist without an evidence row; zero hallucinated stored facts in the real-model run."],
 ["O8: Prevent instructions inside captured text from becoming the user's tasks.", "Partially achieved", "No case executed an action or leaked a secret; twelve injected texts on the untouched half were still stored as readable to-do entries."],
 ["O9: Answer questions from memory with a source for each sentence.", "Partially achieved", "Zero fabricated citations, but 66.7 % correctness and 83.7 % verification against targets of 80 % and 90 %."],
 ["O10: Keep the interface responsive and invisible to screen sharing.", "Achieved", "60 fps, 64 ms input-to-visual, complete absence from two capture paths."],
 ["O11: Make no network connection other than approved model downloads.", "Not achieved as stated", "The application makes none; the WebView2 runtime makes one HTTPS connection per run."],
]),

"C.1": dict(
caption="Configuration reference (%LOCALAPPDATA%\\Tessara\\config.json)",
rows=[
 ["Key", "Type", "Default", "Effect"],
 ["inference.backend", "string", "auto", "Restricts inference to one backend: auto, cuda, rocm, vulkan, sycl, openvino, metal, opencl or cpu."],
 ["inference.device", "string", "(empty)", "Restricts inference to devices matching an identifier, an index or part of a name."],
 ["inference.gpu_layers", "integer", "-1", "Number of layers to offload. -1 offloads all; 0 keeps everything on the CPU."],
 ["inference.allow_fallback", "boolean", "true", "When false, the application refuses to start inference rather than falling back from a forced choice."],
 ["inference.idle_unload_min", "integer", "10", "Minutes of inactivity after which the worker unloads the model. 0 disables unloading."],
 ["capture.cadence_active_s", "integer", "5", "Seconds between checks while the user is active (1 to 60)."],
 ["capture.quality_threshold", "float", "0.55", "Optical-character-recognition quality below which the vision rung is considered."],
 ["storage.retention_days", "integer", "90", "Age at which stored items are pruned (1 to 365)."],
]),

}

TABLES.update({

"6.3": dict(
caption="Redaction results against the seeded secrets corpus",
rows=[
 ["Measure", "Target", "Result"],
 ["Seeded secrets removed", "100 %", "355 / 355 (100 %)"],
 ["False positives on clean text", "\u2264 2 %", "1 / 242 lines (0.41 %)"],
 ["Identifier families covered", "\u2014", "Aadhaar (including virtual identifiers), PAN, passport, voter identity, bank account, IFSC, UPI, payment cards, one-time codes, API keys, JSON web tokens, private keys"],
 ["Behaviour on first run of the 65 new cases", "\u2014", "48 leaked; no detector existed for Aadhaar virtual identifiers, passports or voter identity numbers. Detectors added; redaction version incremented."],
 ["Idempotence", "Stable output", "Defect found by randomised testing (a second one-time code inside the first keyword window survived); redaction now runs to a fixed point."],
]),

"6.4": dict(
caption="Injection corpus results. The held-out half was never used for tuning and is the honest estimate of generalisation.",
rows=[
 ["Run", "Original set (74)", "Tuned half (39)", "Held-out half (39)", "Total (152)"],
 ["Simulated fully compromised model, before audit fixes", "\u2014", "27 / 39", "25 / 39", "126 / 152"],
 ["Simulated, after tuning on the tuned half only", "74 / 74", "39 / 39", "27 / 39 (69 %)", "140 / 152"],
 ["Real Qwen3-VL-4B model", "66 / 74", "33 / 39", "24 / 39 (62 %)", "123 / 152"],
]),

"6.5": dict(
caption="Extraction results on the held-out set, real model run",
rows=[
 ["Measure", "Target", "Result"],
 ["Items processed", "\u2014", "58"],
 ["Gold facts", "\u2014", "83"],
 ["Facts stored", "\u2014", "97, of which 5 held for review and not shown as tasks"],
 ["Precision", "\u2014", "49.5 %"],
 ["Recall", "\u2014", "57.8 %"],
 ["F1", "\u2265 0.70", "0.533 \u2014 not met"],
 ["Hallucinated stored facts", "\u2264 3 %", "0 / 97 = 0.0 % \u2014 met"],
 ["Schema-invalid stored rows", "0", "0 \u2014 met"],
 ["Items lost to invalid output", "\u2014", "0 (15 of 58 before the key-order fix)"],
 ["Injection items with forbidden text stored", "0", "0"],
 ["Wall-clock time for the run", "\u2014", "about 50 minutes (5 h 48 min before the key-order fix)"],
]),

"6.6": dict(
caption="Question answering results over 60 questions",
rows=[
 ["Measure", "Target", "Simulated oracle", "Real model, sorted keys", "Real model, declared key order"],
 ["Answered correctly", "\u2265 80 %", "53 / 60 (88.3 %)", "9 / 60 (15 %)", "40 / 60 (66.7 %) \u2014 not met"],
 ["Shown sentences verified", "\u2265 90 %", "53 / 53 (100 %)", "4 / 4", "41 / 49 (83.7 %) \u2014 not met"],
 ["Fabricated citations", "0", "0", "0", "0 \u2014 met"],
 ["Invalid answers", "0", "0", "3", "0"],
 ["Time to first sentence (p50)", "\u2264 2.5 s", "\u2014", "14.5 s", "25.2 s \u2014 not met"],
 ["Time to full answer (p50)", "\u2264 10 s", "\u2014", "19.2 s", "25.5 s \u2014 not met"],
]),

"6.7": dict(
caption="Draft assistant results over 20 scenarios, real model run",
rows=[
 ["Measure", "Target", "Result"],
 ["Valid drafts produced", "20 / 20", "17 / 20"],
 ["Unsupported claims shown without a flag", "0", "0 \u2014 met"],
 ["Required content present", "\u2014", "14 / 17"],
 ["Forbidden content absent", "\u2014", "16 / 17"],
 ["Placeholder used where expected", "\u2014", "16 / 17"],
 ["Insertion into a password field", "Never", "Refused in a real test \u2014 met"],
 ["Insertion into a changed target", "Never", "Refused in a real test \u2014 met"],
 ["Clipboard restored after insertion", "Always", "All formats restored at original sizes \u2014 met"],
 ["Time to first visible text (p50)", "\u2264 3 s", "5.9 s \u2014 not met"],
 ["Time to complete draft (p50)", "\u2264 8 s", "16.4 s \u2014 not met"],
]),

"6.8": dict(
caption="Interface performance and capture exclusion",
rows=[
 ["Measure", "Target", "Result"],
 ["Animation frame time, p50 / p95 / max", "16.7 ms (60 fps)", "16.7 / 16.8 / 33.3 ms over 1,229 frames in 20 cycles \u2014 met"],
 ["Frames exceeding 17.7 ms", "\u2014", "0.16 %"],
 ["Input-to-visual latency, p50 / p95", "\u2264 100 ms", "33 / 64 ms over 20 opens \u2014 met (292 ms p95 before optimisation)"],
 ["Focus stolen during a peek", "Never", "Foreground unchanged; 20 / 20 focus returns after close \u2014 met"],
 ["Presence in Graphics Capture frames", "Absent", "100 % absent, pill and expanded; control window 100 % visible"],
 ["Presence in GDI block-transfer frames", "Absent", "100 % absent; control window 100 % visible"],
 ["Named conferencing applications", "Absent", "Not run \u2014 outstanding"],
 ["Placement after monitor or scaling change", "Correct", "Unit-tested only; one monitor available \u2014 outstanding"],
]),

"6.9": dict(
caption="Hardware verification coverage",
rows=[
 ["Scenario", "Method", "Outcome"],
 ["Processor only, no GPU", "Real hardware and simulated", "Verified"],
 ["AMD integrated GPU through Vulkan", "Real hardware", "Verified; backend probed, device listed, model executed"],
 ["NVIDIA with CUDA, and NVIDIA falling back to Vulkan", "Simulated adapters and probe output", "Selection logic verified"],
 ["AMD with ROCm, and ROCm failing to Vulkan", "Simulated", "Selection logic verified"],
 ["Intel with SYCL or Vulkan", "Simulated", "Selection logic verified"],
 ["Apple with Metal; Qualcomm with OpenCL", "Simulated", "Selection logic verified"],
 ["Several GPUs present; override by name or index", "Simulated", "Selection logic verified"],
 ["Software rendering adapter present", "Real hardware", "Correctly ignored rather than selected"],
 ["Tampered build directory (extra library planted)", "Real and simulated", "Rejected by the manifest check"],
 ["Probe ignores an inherited device-visibility variable", "Real hardware (AMD, Vulkan)", "Verified"],
 ["Unsupported model and backend combination", "Simulated", "Backend skipped rather than attempted"],
 ["Every backend probe failing", "Simulated; real for a missing executable", "Falls back to the processor"],
]),

})

# Short titles used in the List of Tables.
SHORT = {
"2.1": "Comparison of related approaches",
"3.2": "Non-functional requirements and measured outcomes",
"4.2": "Inference backends and their verification status",
"4.3": "Database tables created by the migrations",
"5.1": "Architecture decision records",
"6.4": "Injection corpus results",
"6.6": "Question answering results over 60 questions",
"6.7": "Draft assistant results over 20 scenarios",
"6.10": "Consolidated scorecard against release criteria",
"7.1": "Project objectives and the evidence for each",
"C.1": "Configuration reference",
}
