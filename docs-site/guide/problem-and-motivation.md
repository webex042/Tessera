# Problem and Motivation

The promise of running foundation models on personal computers is frequently hindered by profound systems-level fragmentation, deployment friction, and security risks. Tessera was conceived to systematically eliminate these engineering obstacles.

---

## 1. Hardware Fragmentation

### Root Cause
Unlike the CPU ecosystem—where standard x86_64 and ARM64 instruction sets provide consistent execution semantics across vendors—the GPU computing landscape is fractured into proprietary vendor stacks:
- **NVIDIA:** Proprietary CUDA runtime and driver ecosystem.
- **AMD:** ROCm / HIP runtimes, historically constrained to Linux discrete cards, with evolving Windows support.
- **Intel:** oneAPI / SYCL runtime stacks for Arc discrete and Xe integrated graphics.
- **Cross-Vendor:** Khronos Vulkan compute and Microsoft DirectML.

### Real-World Consequences
Developers wanting to ship a local AI desktop application must either:
1. Compile five separate versions of their application, each linking against a different vendor SDK.
2. Ship a monolithic 2GB+ distribution bundle containing dynamic libraries for every vendor, which still crashes if the user's driver version has a minor ABI mismatch.
3. Restrict support strictly to NVIDIA GPUs, alienating the majority of laptop and desktop users who rely on AMD or Intel graphics.

### Tessera's Response
Tessera decouples the host application entirely from GPU libraries. The host binary compiles with zero GPU dependencies. At runtime, Tessera discovers installed graphics adapters via OS APIs (DXGI), checks installed builds of `llama.cpp` against strict cryptographic hashes, probes device capabilities with `--list-devices`, and dynamically selects the optimal backend (CUDA, ROCm, SYCL, Vulkan, or CPU).

---

## 2. Process Fragility and Deployment Complexity

### Root Cause
Running large machine learning models pushes consumer hardware to physical memory limits. Allocating 4–6 GB of VRAM for model weights and KV caches frequently triggers Out-of-Memory (OOM) errors, GPU driver resets (TDR), or internal runtime assertions.

### Real-World Consequences
In conventional desktop architectures where inference runs in-process (e.g., via embedded C++ bindings):
- An OOM fault or driver crash unconditionally crashes the host application process.
- Active user documents or databases are corrupted mid-write.
- If inference is outsourced to unmanaged background daemons, crashed workers leave orphaned processes holding gigabytes of VRAM hostage until system reboot.

### Tessera's Response
Tessera isolates model execution into an external sidecar process managed inside a **Windows Job Object** configured with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`. Process communication occurs over loopback TCP (`127.0.0.1`). If the worker crashes or becomes unresponsive:
- The host application UI continues running smoothly.
- A supervisor state machine applies exponential backoff restarts with an hourly restart budget.
- Terminating the parent application causes the Windows kernel to unconditionally terminate the worker child process.

---

## 3. Resource Contention and Idle Drain

### Root Cause
Large language models held resident in memory occupy 4–6 GB of RAM/VRAM and cause GPU compute cores to draw continuous baseline power even when idle.

### Real-World Consequences
- Laptops running on battery suffer dramatic reductions in battery life.
- Foreground 3D games, video editing suites, and CAD software suffer frame drops due to VRAM starvation.

### Tessera's Response
Tessera implements an intelligent, power-aware priority scheduler and supervisor:
- **Priority Tiers:** Interactive user requests (P0) take immediate precedence over background extraction (P1) and daily summaries (P2).
- **Preemption:** If an interactive request arrives during background processing, the scheduler drops the worker's streaming HTTP connection, instantly halting computation.
- **Power Awareness:** When the system switches to battery power, background processing pauses, and the worker process automatically unloads from memory after 10 minutes of inactivity.

---

## 4. Privacy Violations and Prompt Injection

### Root Cause
Desktop memory tools that read visible screen contents inevitably capture sensitive credentials, bank accounts, passwords, and private messages. Furthermore, untrusted text displayed in web browsers or emails can contain adversarial instructions designed to hijack language model behavior (indirect prompt injection).

### Real-World Consequences
- Unencrypted databases store plaintext passwords and personal identity numbers.
- Injected instructions command the model to delete files, exfiltrate data, or fabricate false commitments.

### Tessera's Response
- **Pre-Storage Redaction:** Regex patterns combined with algorithmic checksums (Luhn, Verhoeff) redact credit cards, identity numbers, and API keys before persistence.
- **Air-Gapped Network Boundary:** All inference and search run locally; outbound networking is restricted to a single crate (`tsr-net`) requiring explicit user approval.
- **Data vs. Authority Separation:** Document text is wrapped in `<screen_content>` blocks. The model has zero tool-calling privileges, and its JSON output is validated deterministically by Rust code.

---

## Problem-to-Impact Matrix

| Problem Area | Affected Users | Technical Consequence | Tessera Architectural Response | Residual Limitation |
|---|---|---|---|---|
| **GPU Vendor Lock-In** | All developers & non-NVIDIA users | Broken builds; runtime driver crashes | Pinned sidecar architecture + dynamic DXGI probing | Real execution tested so far on AMD Vulkan & CPU; others simulated |
| **Inference Crashes & OOM** | Desktop end-users | App termination; data corruption | Windows Job Objects + Supervisor lifecycle state machine | Model unavailable if restart budget (5/hr) is exceeded |
| **VRAM & Battery Drain** | Laptop users & gamers | Battery depletion; 3D stutter | Preemptive priority scheduler + idle unload on battery | Cold reload takes 5–15 seconds when reactivation occurs |
| **Screen Secret Leaks** | Privacy-conscious users | Credentials persisted to disk | Multi-rung masking + pre-storage regex/checksum redaction | OCR can occasionally misread unusual fonts or symbols |
| **Prompt Injection** | Desktop knowledge workers | Compromised model actions | Rigid delimiters + zero-tool model contract + Rust fact validation | Compromised model can still produce distorted extractive text |
