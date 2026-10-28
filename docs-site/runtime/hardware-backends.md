# Hardware Backends

Tessera does not depend on any single GPU vendor. No crate in the workspace links directly to CUDA, ROCm, Vulkan, SYCL, OpenVINO, Metal, or OpenCL. Compute acceleration is achieved via discrete, pinned `llama.cpp` (b11900) builds arbitrated at runtime.

---

## Supported Backend Matrix

| Backend | Hardware Architecture | Windows x64 Package | Linux x64 Package | Driver Prerequisites |
|---|---|---|---|---|
| **CPU** | x86_64 / ARM64 (AVX2/AVX-512) | `cpu-x64` | `linux-x64` | Standard OS scheduler (No GPU required). |
| **CUDA** | NVIDIA GeForce, RTX, Quadro | `cuda-12.4-x64` + `cudart-12.4-x64` | `linux-cuda-12.8-x64` | NVIDIA Driver $\ge 551.00$ (CUDA 12.4). |
| **Vulkan** | AMD, Intel, NVIDIA, Qualcomm | `vulkan-x64` | `linux-vulkan-x64` | GPU display driver supporting Vulkan 1.2+ (`vulkan-1.dll`). |
| **ROCm (HIP)**| AMD Discrete GPUs (RDNA2+) | `rocm-x64` | `linux-rocm-x64` | AMD Adrenalin with HIP runtime support. |
| **SYCL** | Intel Arc, Xe Graphics | `sycl-x64` | `linux-sycl-fp16-x64` | Intel Graphics Driver (oneAPI runtime included). |
| **OpenVINO** | Intel NPU / GPU | `openvino-x64` | `linux-openvino-x64` | Intel Driver (*experimental, manual override only*). |
| **Metal** | Apple Silicon (M1/M2/M3/M4) | — | — | macOS 12+ on Apple Silicon (Architecture ready). |
| **OpenCL** | Qualcomm Adreno (Snapdragon X)| `opencl-adreno-arm64` | — | Qualcomm Snapdragon X Compute Driver. |

::: warning Unsupported Platforms
The following configurations are explicitly not supported:
- Intel-based Apple Macs (macOS x64 build runs CPU only).
- DirectML (omitted due to operator performance overhead in llama.cpp).
- Obsolete GPUs lacking Vulkan 1.2 or modern compute runtimes (fall back automatically to CPU).
:::

---

## Real Hardware vs. Simulated Verification

Tessera maintains strict empirical distinction between backends tested on physical silicon and those verified via simulated test suites:

- **AMD + Vulkan (Real Hardware):** Verified on AMD Ryzen 5 7530U with Radeon Graphics (b11900 Vulkan build). Successfully allocates model in shared system RAM and performs end-to-end token generation.
- **CPU Fallback (Real Hardware):** Verified with AVX2/FMA execution on Windows 11.
- **CUDA, ROCm, SYCL, Metal, OpenCL (Simulated):** Selection logic, adapter matching, and probe parsing are verified against mocked hardware outputs matching exact vendor strings (`crates/tsr-infer/tests/selection.rs`).
