# What is Tessera?

Tessera is an AI systems platform and local-first memory application for Windows. It provides infrastructure for running multimodal artificial intelligence locally while using that infrastructure to create a private, screen-aware memory assistant.

---

## The Dual Nature of Tessera

Tessera combines two tightly integrated engineering achievements:

### 1. Vendor-Independent Local Inference Infrastructure
At the systems layer, Tessera investigates how local language and vision models can execute portably across diverse consumer hardware without requiring application developers to bundle massive, proprietary toolchains:
- **Zero Vendor SDK Linking:** The core application binary links neither to CUDA, ROCm, Vulkan, SYCL, nor Metal.
- **Process Isolation:** The inference engine runs in a dedicated child process (`llama-server`) encapsulated within an operating-system Job Object.
- **Dynamic Backend Arbitration:** Hardware adapters are discovered via OS query APIs, verified against cryptographic manifests, and probed at runtime to select the highest-performing compute backend with automatic fallback down to the CPU.

### 2. A Screen-Aware Desktop Memory System
At the application layer, Tessera applies this inference layer to personal desktop context:
- **Continuous Attention:** Observes active applications and documents using an adaptive capture ladder (UI Automation $\rightarrow$ Windows Graphics Capture + OCR $\rightarrow$ Vision-Language Model).
- **Verifiable Citations:** Every memory item, morning brief, or answered question links directly back to exact source text spans and capture timestamps.
- **In-Place Drafting:** Generates context-grounded email and message replies directly into the user's active window, restoring clipboard state and enforcing strict fact-checking before text insertion.
- **Cryptographic Local Persistence:** All captured text and extracted facts reside in an encrypted SQLCipher database protected by Windows DPAPI.

---

## How Tessera Differs from Existing Approaches

| Architectural Dimension | Conventional Cloud AI (e.g. Copilot, ChatGPT) | Standard Local Wrappers (e.g. Electron + Ollama) | Tessera Architecture |
|---|---|---|---|
| **Data Locality** | Screen content and prompts transmitted to cloud data centers. | Local, but often lacks strict outbound network sandboxing. | **Strictly Air-Gapped:** Zero network calls in core crates; verified compile-time boundary (`tsr-net`). |
| **Process Containment** | N/A (Server-side execution). | In-process or uncontained background daemons that persist on crash. | **Job Object Encapsulation:** Guaranteed kill-on-close; port-owner authenticated loopback. |
| **Hardware Portability** | N/A (Hardware handled by cloud provider). | Bundles one massive binary or relies on manual user terminal setup. | **Dynamic Runtime Probing:** Manifest-checked builds arbitrated automatically at runtime. |
| **Safety & Redaction** | Cloud filters apply after data has already left the machine. | Often passes raw screen buffers directly to the model. | **Deterministic Pre-Storage Redaction:** Regex + Luhn/Verhoeff checksums scrub secrets before disk. |
| **Model Grounding** | Unverifiable citations; susceptible to hallucination. | Basic RAG; answers can freely invent unsupported statements. | **Strict Substring Evidence:** Output rejected unless quote exists in source; 0% hallucination rate. |

---

## Current Development Phase

Tessera is currently midway through its primary development plan (Stages 0–7). Core infrastructure—capture hooks, encrypted storage, redaction, sidecar supervision, and scheduler preemption—is fully functional and verified by automated test suites. Ongoing work focuses on improving the accuracy of small 4B vision-language models and resolving an open telemetry connection originating inside the upstream Microsoft WebView2 runtime.
