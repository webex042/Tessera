# ADR-0004: GPU-vendor-independent inference

- Status: Accepted (2026-10-26)
- Revisits: plan header "Target hardware", D2, D3, §4.1, §7.1, §7.2, §22 Q1 and Q8

## Context

The plan targeted one reference machine (NVIDIA RTX 4050, 6 GB). The only development machine
available has an AMD Radeon iGPU, and users will have every kind of GPU or none. An audit found
no Rust dependency on CUDA (the sidecar design already isolated it), but the benchmark measured
GPU memory only with `nvidia-smi`, the docs assumed NVIDIA, and there was no detection or
backend-selection code at all.

## Decision

1. **Backends live outside Tessara's binary.** Each llama.cpp backend is a separate pinned build
   (`tools/sidecar.lock`): CPU, CUDA, Vulkan, ROCm, SYCL, OpenVINO, Metal, OpenCL. Tessara links
   none of their libraries. A build is trusted only if every file matches its manifest.
2. **Detect, probe, rank, fall back** (`crates/tsr-infer`): OS adapter enumeration (no GPU
   runtime), a real `--list-devices` probe per verified build, GGUF model checks, a ranked
   candidate list with CPU last, and start-with-fallback. User override: backend, device,
   GPU layers, allow-fallback.
3. **The model is never changed for compatibility.** No automatic re-quantisation or model swap;
   unverified model/backend pairs are skipped (or used only when forced, with a warning).
4. **Nothing is downloaded implicitly.** Builds are fetched only by a person running
   `tools/fetch_sidecar.py`; first use of a build requires a recorded, reviewed manifest.
5. The RTX 4050 stays the **reference** configuration for the plan's numeric gates (5.2 GB), and
   Stage 0 model gates are re-run per backend that will ship.

## Consequences

- Tessara builds and runs (CPU) on a machine with no GPU and no vendor SDK; CI exercises that path.
- Shipping every backend would make the installer large (CUDA runtime alone is ~390 MB); the
  installer question (plan §22 Q1) becomes "which builds to bundle vs. offer as optional,
  consented downloads" — decided in Stage 10.
- Only CPU and AMD+Vulkan have run on real hardware so far; other vendors are covered by simulated
  tests until testers with that hardware run `tsr-hw` and the real-hardware suite (Stage 9).
