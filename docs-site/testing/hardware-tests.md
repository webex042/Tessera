# Hardware Validation Suite

Hardware detection and arbitration in `tsr-infer` are verified through complementary simulated and physical test suites.

---

## 1. Simulated Device Matrix (`selection.rs`)

The test suite in `crates/tsr-infer/tests/selection.rs` evaluates candidate selection across realistic multi-GPU environments without requiring physical hardware:

| Test Scenario | Simulated Hardware Configuration | Verified Behavior |
|---|---|---|
| `nvidia_cuda_preferred` | NVIDIA RTX 4070 Laptop (8GB VRAM) | Selects CUDA candidate; falls back to Vulkan if CUDA build missing. |
| `amd_rocm_fallback` | AMD Radeon RX 7800 XT (16GB VRAM) | Tries ROCm; falls back to Vulkan if HIP runtime absent. |
| `intel_sycl_priority` | Intel Arc A770 (16GB VRAM) | Prioritizes SYCL over Vulkan; OpenVINO skipped unless forced. |
| `dual_gpu_discrete_first`| Discrete RTX 4050 (6GB) + Integrated AMD Radeon (512MB) | Selects discrete GPU over iGPU despite iGPU reporting shared system RAM. |
| `software_renderer_ignored`| Microsoft Basic Render Driver | Skips software adapter unconditionally; reverts to CPU. |
| `unsupported_quant_skipped`| IQ2_XXS on older Vulkan driver | Detects unsupported tensor type in GGUF header; skips Vulkan. |

---

## 2. Physical Silicon Suite (`real_hardware.rs`)

Located at `crates/tsr-infer/tests/real_hardware.rs`:
- Tests actual DXGI adapter enumeration against the Windows display driver stack.
- Executes `llama-server --list-devices` with a stripped environment to verify child process execution.
- Verifies that inherited environment variables (e.g., `GGML_VK_VISIBLE_DEVICES`) are stripped cleanly.
- Tests token generation against real CPU and Vulkan backends.
