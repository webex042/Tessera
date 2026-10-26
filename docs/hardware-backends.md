# Hardware and inference backends

Tessara does not depend on any GPU vendor. No Tessara crate links to CUDA, ROCm, Vulkan, Metal,
SYCL, OpenVINO or OpenCL. GPU support comes from **separately pinned llama.cpp builds**, one per
backend, and the backend is chosen at run time (ADR-0004). Without any GPU, Tessara runs on the
CPU. Nothing GPU-specific is downloaded unless you run the fetch command for that build yourself.

## Supported backends (llama.cpp b11900)

| Backend | GPUs | Windows x64 | Windows arm64 | Linux x64 | macOS arm64 | Needs on the machine |
|---|---|---|---|---|---|---|
| CPU | none needed | `cpu-x64` | `cpu-arm64` | `linux-x64` | (in `macos-arm64`) | nothing |
| CUDA | NVIDIA | `cuda-12.4-x64` + `cudart-12.4-x64` | — | `linux-cuda-12.8-x64` + cudart | — | NVIDIA driver (CUDA 12.4: driver ≥ 551) |
| Vulkan | NVIDIA, AMD, Intel, others | `vulkan-x64` | `vulkan-arm64` | `linux-vulkan-x64` | — | GPU driver with Vulkan 1.2+ (`vulkan-1.dll` / libvulkan) |
| ROCm (HIP) | AMD (RDNA2+ discrete; see AMD's ROCm support list) | `rocm-x64` | — | `linux-rocm-x64` | — | AMD driver with HIP runtime |
| SYCL | Intel (Arc, Xe iGPU, Data Center) | `sycl-x64` | — | `linux-sycl-fp16-x64` | — | Intel GPU driver (oneAPI runtime is in the build) |
| OpenVINO | Intel | `openvino-x64` | — | `linux-openvino-x64` | — | Intel driver; **newer backend, used only when forced** |
| Metal | Apple Silicon | — | — | — | `macos-arm64` | macOS on M-series |
| OpenCL | Qualcomm Adreno | — | `opencl-adreno-arm64` | — | — | Snapdragon X driver |

Not supported: Intel Macs (the macOS x64 build is CPU only), DirectML, and any GPU without one of
the drivers above (it falls back to CPU).

### Automatic choice per vendor (first that initialises wins)

| Vendor | Order | Then |
|---|---|---|
| NVIDIA | CUDA → Vulkan | CPU |
| AMD | ROCm → Vulkan | CPU |
| Intel | SYCL → Vulkan (→ OpenVINO only if forced) | CPU |
| Apple | Metal | CPU |
| Qualcomm | OpenCL → Vulkan | CPU |
| Other | Vulkan | CPU |
| Software renderer (WARP, llvmpipe) | never used | CPU |

Across several GPUs: a device that fits the whole model beats partial offload; a discrete GPU
beats an integrated one (even if the integrated one reports more shared memory); then the vendor's
preferred backend; then the most free memory.

## How selection works

1. **Adapters (no GPU runtime loaded):** DXGI on Windows, `/sys/class/drm` on Linux, the Apple GPU
   on Apple Silicon. Gives vendor and memory.
2. **Verified builds:** each build directory under `third_party/llama_cpp/<tag>/` must match its
   manifest in `tools/sidecar.lock` exactly — every file's SHA-256, and no extra files. The lock
   is compiled into Tessara. (All builds share the same `llama-server.exe`; the GPU code is in
   `ggml-<backend>.dll`, so pinning only the executable would not be enough.)
3. **Backend probe:** each verified GPU build runs `llama-server --list-devices` with a cleared
   environment and a 20 s timeout. This initialises the backend for real and lists usable devices
   and free memory, without loading a model.
4. **Model check:** the GGUF header (format version, architecture, tensor types, size) is read
   without touching tensor data. Tensor types a backend is not known to handle are "unverified";
   that backend is then skipped unless you force it. The model is **never** re-quantised or
   swapped to make it fit.
5. **Memory:** whole model fits → full offload; fits without the vision encoder → vision encoder
   on CPU (`--no-mmproj-offload`); else partial offload if at least a quarter of the layers fit;
   else skip.
6. **Fallback:** candidates are tried in order; if a backend fails to start, the next one is tried
   and the reason is logged. CPU is always the last candidate unless you disable fallback.

Every decision is logged (`tracing`, content-free): adapter list, each probe result, the selected
backend/device with memory and reason, and every skipped option or failed start with its reason.

### Manual override

Settings (to be exposed in the settings window, Stage 3):

| Setting | Values | Effect |
|---|---|---|
| `inference.backend` | `auto` (default), `cuda`, `rocm`/`hip`, `vulkan`, `sycl`, `openvino`, `metal`/`mps`, `opencl`, `cpu` | Use only this backend. Also allows pairings and model types that are not chosen automatically (logged as a warning). |
| `inference.device` | device id (`CUDA1`, `Vulkan0`), index (`1`), or part of the name (`rtx 4050`) | Use only matching devices. |
| `inference.gpu_layers` | number | Offload exactly this many layers (`0` = CPU). |
| `inference.allow_fallback` | `true` (default) / `false` | `false`: if the forced choice is impossible, refuse to start instead of falling back. |

## What has been tested

| Scenario | How | Where |
|---|---|---|
| No GPU / CPU only | simulated + **real** (CPU build) | `crates/tsr-infer/tests/selection.rs`, `real_hardware.rs` |
| NVIDIA + CUDA (and NVIDIA without CUDA build → Vulkan) | **simulated** | `selection.rs` |
| AMD + ROCm; AMD where ROCm fails → Vulkan | **simulated** | `selection.rs` |
| AMD + Vulkan | **real**: AMD Radeon (Ryzen 5 7530U iGPU), Vulkan build b11900 | `real_hardware.rs`, `tsr-hw` |
| Intel + SYCL / Vulkan | **simulated** | `selection.rs` |
| Apple + Metal | **simulated** | `selection.rs` |
| Qualcomm + OpenCL | **simulated** | `selection.rs` |
| OS GPU detection failure | simulated | `selection.rs` |
| Every backend probe failing | simulated; **real** for a missing executable | `selection.rs`, `real_hardware.rs` |
| GPU start failure → next backend → CPU | simulated (scripted launcher) | `selection.rs` |
| Unsupported model/backend combination | simulated (IQ2_XXS on Vulkan) | `selection.rs` |
| Several GPUs (dGPU + iGPU, two dGPUs, override by name/index) | simulated | `selection.rs` |
| Software renderer ignored | simulated; **real**: Microsoft Basic Render Driver skipped on dev machine | `selection.rs`, `tsr-hw` |
| Probe ignores inherited env (`GGML_VK_VISIBLE_DEVICES`) | **real** (Vulkan, AMD) | `real_hardware.rs` |
| Tampered build (extra DLL) rejected | simulated + real (planted DLL) | `backend.rs` tests, `fetch_sidecar.py` |
| Per-process GPU memory measurement (any vendor) | **real** (AMD, dwm.exe) | `bench/tests/test_harness.py` |

"Simulated" means mocked adapter lists and mocked probe output in the exact format real builds
print. It proves the selection logic, not that a given vendor's driver works. **Only AMD + Vulkan
and CPU have run on real hardware.** No model has been run on any backend yet (no model files on
the dev machine by the owner's decision), so inference speed and memory per backend are unmeasured.

## Installation

Pick the build(s) for your hardware and fetch them (each download is pinned and hash-checked;
nothing is fetched automatically):

```bash
python tools/fetch_sidecar.py --list
python tools/fetch_sidecar.py llama_cpp cpu-x64
python tools/fetch_sidecar.py llama_cpp vulkan-x64
```

A build whose file manifest is not recorded yet (`files: null` in the lock — all except `cpu-x64`
and `vulkan-x64` today) must be recorded once on a trusted machine, reviewed, and committed:

```bash
python tools/fetch_sidecar.py llama_cpp cuda-12.4-x64 --record
python tools/fetch_sidecar.py llama_cpp cudart-12.4-x64
python tools/fetch_sidecar.py llama_cpp cuda-12.4-x64 --record
```

(The CUDA runtime archive unpacks into the CUDA build's folder, so the CUDA manifest is recorded
again after adding it.) Until a manifest is committed, Tessara refuses to use that build.

## Verification on a machine with a GPU

```bash
# 1. What does this machine have, and what would Tessara pick?
cargo run -p tsr-infer --bin tsr-hw
cargo run -p tsr-infer --bin tsr-hw -- --json

# 2. With a real model (headers only are read):
cargo run -p tsr-infer --bin tsr-hw -- --model <path>/Qwen3-VL-4B-Instruct-Q4_K_M.gguf --mmproj <path>/mmproj-Qwen3-VL-4B-Instruct-F16.gguf

# 3. Force a backend / device to check each one:
cargo run -p tsr-infer --bin tsr-hw -- --backend vulkan --device 0
cargo run -p tsr-infer --bin tsr-hw -- --backend cuda --no-fallback

# 4. Real-hardware test suite:
cargo test -p tsr-infer --test real_hardware -- --ignored --nocapture --test-threads=1

# 5. Benchmark the chosen backend (needs model files; device/ngl from step 1):
python bench/run_bench.py --server third_party/llama_cpp/b11900/<variant>/llama-server.exe --model <...> --mmproj <...> --device <ID> --ngl all --label <name>
```

## Limitations

- **Only CPU and AMD+Vulkan verified on real hardware.** CUDA, ROCm, SYCL, OpenVINO, Metal and
  OpenCL selection logic is tested with simulated devices only.
- **Model support per backend is a conservative table**, not a measurement. ggml runs operations a
  backend lacks on the CPU (slower, same results), so "unverified" types are skipped for speed and
  safety, not because outputs would change. Results can still differ slightly between backends in
  floating point; evals must be recorded per backend.
- **Integrated GPUs use shared system memory.** On the dev machine Vulkan reports 8 GB on a GPU
  with 512 MB "dedicated" memory; offloading there competes with the OS for RAM.
- **Integrity check cost:** detection hashes every file of every installed build. Measured on the
  dev machine with CPU + Vulkan builds installed (105 files): whole detection incl. the Vulkan probe
  **0.6–0.7 s in a release build**, 10.4 s in a debug build (unoptimised SHA-256). Larger builds
  (CUDA runtime ~390 MB) will cost more; a cached check (size + modification time) is a Stage 2
  option if needed.
- **GPU memory measurement** in the benchmark uses Windows counters (any vendor, ~1 sample per
  1.5–2 s); on Linux/macOS it reports nothing yet. `nvidia-smi` is used additionally when present.
- **ROCm on Windows** supports only some AMD GPUs; integrated Vega/RDNA iGPUs are generally not
  among them, which is why Vulkan is the AMD fallback.
- **OpenVINO** and **Adreno OpenCL** backends are new in llama.cpp; Tessara never picks OpenVINO
  automatically and trusts only F16/F32/Q4_0/Q8_0 on both.
- The plan's 5.2 GB memory gate is defined for the 6 GB reference GPU. Other GPUs are judged by
  whether the plan's chosen offload fits their reported free memory.
