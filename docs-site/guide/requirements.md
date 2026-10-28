# System Requirements

This document specifies the target hardware profiles, operating system dependencies, and compiler toolchains required to build and operate Tessera.

---

## Hardware Profiles

### Reference Configuration (Target Gate)
The quantitative gates in Tessera's architecture plan (§1.3) are calibrated against a typical modern mid-range creator/gaming laptop:
- **Operating System:** Windows 11 (64-bit).
- **GPU:** Dedicated NVIDIA GeForce RTX 4050 Laptop GPU (6 GB VRAM).
- **System Memory:** 16 GB RAM.
- **Model VRAM Allocation Budget:** 4.2 GB – 5.2 GB (accommodates Q4_K_M model weights, F16 vision projector, 8k context KV cache, and compute buffers).
- **Expected Latency:** Time-to-first-token $\le 2.5\text{s}$, full generation $\le 8.0\text{s}$.

### Minimum Verified Configuration (Dev Environment)
Tessera compiles and executes on commodity integrated graphics without a discrete GPU:
- **Processor:** AMD Ryzen 5 7530U (6 cores / 12 threads with AVX2, FMA, F16C).
- **Graphics:** AMD Radeon Graphics (Integrated iGPU, Vulkan 1.3).
- **System Memory:** 16 GB shared LPDDR4x RAM (dynamically allocated VRAM).
- **Status:** Functional. Runs all capture, store, and inference pipelines, but generation latency exceeds interactive targets ($14.5\text{s}$ first token).
- **Operational Mode:** Extraction runs only when connected to AC power and the system is idle.

### CPU-Only Fallback
On machines lacking a compatible GPU driver, Tessera automatically falls back to CPU execution:
- **Processor:** Any x86_64 CPU supporting AVX2.
- **Memory:** Minimum 12 GB available system RAM.
- **Status:** Fully functional; slow mode for background extraction.

---

## Operating System & Runtime Dependencies

| Dependency | Required Version | Verification Command | Notes |
|---|---|---|---|
| **Windows 11** | Build 22000+ (64-bit) | `winver` | Required for borderless WGC, WinRT OCR, and DPAPI key wrapping. |
| **WebView2 Runtime** | Evergreen (latest) | Auto-installed with Win 11 | Renders desktop UI shell (`tessara.exe`). |
| **GPU Display Driver** | Vendor-dependent | `dxdiag` | NVIDIA $\ge 551.00$ (CUDA 12.4); AMD Adrenalin with Vulkan 1.3+; Intel oneAPI driver. |
| **`vulkan-1.dll`** | Vulkan 1.2+ | In `System32` | Installed automatically with modern display drivers. |

---

## Build & Toolchain Prerequisites

| Toolchain / Software | Required Version | Purpose |
|---|---|---|
| **Rust & Cargo** | Stable (MSVC) `1.99+` / Edition 2024 | Core application and crates. |
| **Visual Studio Build Tools** | 2022 (with C++ Workload) | MSVC linker and Windows SDK headers. |
| **Node.js** | v24.x | Frontend build environment. |
| **pnpm** | v12.15.0 | Workspace package management. |
| **Python** | v3.11+ | Evals runners, benchmarks, and dependency checkers. |
| **Strawberry Perl** | Portable or installed | Required to compile SQLCipher's vendored OpenSSL. |
