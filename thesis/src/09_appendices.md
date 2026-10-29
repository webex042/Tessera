# APPENDIX A|PROJECT DIRECTORY STRUCTURE

```
Tessera/
├── crates/
│   ├── tsr-core/        Domain primitives, Sensitive<T>, content-free logging
│   ├── tsr-win/         Win32 and WinRT wrappers: DPAPI, Job Objects, hooks,
│   │                    display affinity, clipboard, TCP table queries
│   ├── tsr-capture/     Acquisition ladder (UIA, graphics capture, OCR, vision),
│   │                    deadline worker, SimHash, sessions, store writer
│   ├── tsr-guard/       Blocklists, redaction engine, claim checker
│   ├── tsr-store/       SQLCipher store, migrations, FTS5, repositories
│   ├── tsr-retrieval/   Pinned ONNX runtime, embeddings, chunking,
│   │                    hybrid search, Ask pipeline
│   ├── tsr-infer/       Hardware discovery, GGUF parsing, backend selection,
│   │                    sidecar supervisor, loopback client, scheduler
│   ├── tsr-net/         Approval-gated, digest-verified model downloads
│   ├── tsr-pipeline/    Extraction, schema, evidence, injection guard,
│   │                    date and people resolvers, rollups, briefs, loops
│   └── tsr-draft/       Draft generation and independent fact checking
├── src-tauri/           Desktop shell: windows, tray, hotkeys, typed IPC
├── ui/                  React and TypeScript notch interface
├── bench/               Acquisition probes and performance harnesses
├── evals/               tsr-eval harness, datasets and recorded results
├── tests/fixtures/      Secrets corpus, injection corpus, screen fixtures
├── tools/               Pinned manifests, dependency checker, scan scripts
├── third_party/         Fetched llama.cpp builds and ONNX Runtime
└── docs/                Decision records, stage verification reports,
                         privacy and security documentation
```

# APPENDIX B|INSTALLATION AND DEVELOPMENT SETUP

Prerequisites are listed in Table 3.3. The steps below assume a Windows 11 machine with the Rust MSVC toolchain, Visual Studio C++ build tools, Strawberry Perl, Node.js with pnpm, and Python 3.11 or newer already installed.

```
# 1. Frontend dependencies and interface bundle
pnpm install
pnpm build:ui

# 2. Fetch pinned inference runtimes for this machine's hardware.
#    Each download is hash-checked against tools/sidecar.lock.
python tools/fetch_sidecar.py --list
python tools/fetch_sidecar.py llama_cpp cpu-x64
python tools/fetch_sidecar.py llama_cpp vulkan-x64

# 3. Fetch the pinned embedding runtime
./tools/fetch-sidecar.ps1 -Component onnxruntime -Variant win-x64

# 4. Full test suite, including tests that need a real desktop
cargo test --workspace -- --include-ignored

# 5. Verify the dependency boundary and the evaluation fixtures
python tools/check_network_deps.py --locked
python evals/validate.py

# 6. Build the desktop binary
cargo build -p tessara --features custom-protocol

# 7. Inspect what hardware would be selected on this machine
cargo run --release -p tsr-infer --bin tsr-hw -- \
    --model <path>/Qwen3VL-4B-Instruct-Q4_K_M.gguf \
    --mmproj <path>/mmproj-Qwen3VL-4B-Instruct-F16.gguf
```

Model files are not bundled and are never fetched automatically. They are downloaded only through an explicit approval step and are accepted only when their SHA-256 digest matches the pinned manifest in `tools/models.lock`.

!!PLACEHOLDER: The repository's licence is recorded as "to be determined". A licence should be chosen and stated before the thesis is submitted, since the repository is referenced as project evidence.

!!PLACEHOLDER: The repository folder and documentation use the name *Tessera*, while the compiled binary, the data directory (`%LOCALAPPDATA%\Tessara`) and the Cargo package are named `tessara`. The thesis uses *Tessera* throughout, and quotes the internal identifier exactly where a path or package name is given. Please confirm which spelling is intended, and whether the internal identifier should be renamed before submission.

# APPENDIX C|CONFIGURATION REFERENCE

Table C.1 lists every configuration key, its type, its default and its effect.

TBL:C.1

# APPENDIX D|SELECTED TEST CASES

The following cases are representative of the test suite and are each cited in Chapter 6.

**D.1 — A read never blocks the capture loop.** *Purpose:* confirm that an accessibility read against an unresponsive application cannot stall capture. *Method:* read sixteen windows across nine applications with the production deadline worker, recording both the time the caller waited and the time the underlying read took. *Expected:* caller wait no greater than 300 ms. *Observed:* maximum 294 ms; abandoned reads continued for up to 732 ms in the background. *Interpretation:* the deadline, not the application, bounds the caller.

**D.2 — A wrong key writes nothing to standard error.** *Purpose:* confirm that the encryption library's own diagnostics cannot leak database content. *Method:* open the database with an incorrect key in a child process and capture its standard error stream. *Expected:* empty. *Observed:* empty. *Interpretation:* cipher logging is disabled as intended.

**D.3 — Generic words do not over-block.** *Purpose:* confirm that the blocklist does not suppress ordinary windows. *Method:* evaluate non-browser windows whose titles contain "login", "sign in" and "password". *Expected:* captured. *Observed:* captured. *Interpretation:* the title rule is scoped to browsers, so a developer's editor remains visible to the system.

**D.4 — A fact cannot outlive its evidence.** *Purpose:* confirm the provenance guarantee at the storage layer. *Method:* delete the last evidence row of a fact. *Expected:* the fact row disappears. *Observed:* deleted by trigger. *Interpretation:* provenance is enforced by the schema rather than by application code.

**D.5 — No code path closes a commitment automatically.** *Purpose:* confirm that nothing is marked done without the user. *Method:* a source scan that fails if any path other than the approval-token command writes a closed status, together with behavioural tests covering completion evidence, stated plans and other people's completions. *Expected:* no such path. *Observed:* none. *Interpretation:* automatic closure is structurally prevented, not merely avoided.

**D.6 — Insertion refuses a changed or protected target.** *Purpose:* confirm draft insertion safety. *Method:* attempt insertion into a password field, into a different focused element, and into an element whose password state cannot be read. *Expected:* all three refused. *Observed:* all three refused. *Interpretation:* the unknown case is treated as the dangerous one.

**D.7 — The interface is absent from screen captures.** *Purpose:* confirm display-affinity exclusion. *Method:* place a magenta backdrop beneath the window and capture the monitor through Windows Graphics Capture and through GDI block transfer, with an unexcluded control window and an excluded control window in the same frames. *Expected:* the interface absent, the unexcluded control present, the excluded control absent. *Observed:* exactly that, for both the compact and expanded states. *Interpretation:* the mechanism works at the operating-system level; the named conferencing applications remain untested.

**D.8 — The backend probe ignores an inherited environment.** *Purpose:* confirm that the inference worker cannot be reconfigured by the parent's environment. *Method:* set a device-visibility variable in the parent and run the probe. *Expected:* the variable has no effect. *Observed:* no effect; a control run with the variable passed deliberately showed the opposite. *Interpretation:* environment stripping works.

# APPENDIX E|GLOSSARY

**Backend.** A compiled build of the inference engine that targets one kind of acceleration hardware, such as CUDA or Vulkan. In Tessera a backend is an external artefact, not a linked library.

**BM25.** A ranking function for keyword search that rewards rare query terms and penalises unusually long documents.

**Content-free logging.** A logging discipline in which only allowlisted message strings and field names are emitted, so that user content cannot reach a log file even by mistake.

**Dice coefficient.** A similarity measure between two sets, twice the size of their intersection divided by the sum of their sizes. Used here for fuzzy quotation matching and for deduplication.

**Display affinity.** A Windows window attribute that excludes a window from screen capture and screen sharing.

**GGUF.** The container format used by llama.cpp for model weights; its header describes the architecture, layer count and quantisation of each tensor.

**Indirect prompt injection.** An attack in which adversarial instructions are placed in content that an application retrieves and feeds to a language model, rather than typed by the user.

**Job Object.** A Windows mechanism that groups processes so that limits apply collectively; used here to guarantee that the inference worker dies with the application.

**Quantisation.** Reducing the number of bits used to represent each model weight, trading accuracy for size and speed.

**Reciprocal Rank Fusion.** A method of combining several ranked lists using only the positions in each list rather than their scores.

**Rung.** One stage of Tessera's three-stage text acquisition ladder.

**Sidecar.** A separate process that performs work on behalf of an application, isolated from it so that its failures are contained.

**SimHash.** A fingerprint whose Hamming distance between two documents is small when the documents are similar; used for near-duplicate detection.

**UI Automation.** The Windows accessibility framework, which exposes other applications' interface elements and their text.

**Vision-language model.** A model that accepts images alongside text.

# APPENDIX F|LIST OF ABBREVIATIONS

**ADR** Architecture Decision Record · **AES** Advanced Encryption Standard · **API** Application Programming Interface · **AVX** Advanced Vector Extensions · **BM25** Best Matching 25 · **CPU** Central Processing Unit · **CUDA** Compute Unified Device Architecture · **DPAPI** Data Protection Application Programming Interface · **DXGI** DirectX Graphics Infrastructure · **FTS** Full-Text Search · **GDI** Graphics Device Interface · **GGUF** GPT-Generated Unified Format · **GPU** Graphics Processing Unit · **HIP** Heterogeneous-Compute Interface for Portability · **HTTP** Hypertext Transfer Protocol · **IFSC** Indian Financial System Code · **IPC** Inter-Process Communication · **JSON** JavaScript Object Notation · **MVP** Minimum Viable Product · **NPU** Neural Processing Unit · **OCR** Optical Character Recognition · **ONNX** Open Neural Network Exchange · **OTP** One-Time Password · **OWASP** Open Worldwide Application Security Project · **PAN** Permanent Account Number · **PII** Personally Identifiable Information · **RAG** Retrieval-Augmented Generation · **RRF** Reciprocal Rank Fusion · **SBOM** Software Bill of Materials · **SQL** Structured Query Language · **SYCL** (a Khronos heterogeneous programming standard) · **TCP** Transmission Control Protocol · **TLS** Transport Layer Security · **UIA** UI Automation · **UPI** Unified Payments Interface · **VID** Virtual Identifier · **VLM** Vision-Language Model · **VRAM** Video Random-Access Memory · **WAL** Write-Ahead Logging · **WGC** Windows Graphics Capture · **WinRT** Windows Runtime
