# Backend Selection Matrix

Tessera selects hardware execution backends at runtime using a deterministic 6-step arbitration algorithm implemented in `crates/tsr-infer/src/select.rs`.

---

## The 6-Step Selection Algorithm

```
Step 1: Enumerate OS Adapters (DXGI / sysfs) without loading vendor runtimes
  │
Step 2: Verify installed sidecar builds against SHA-256 hashes in sidecar.lock
  │
Step 3: Probe intact builds via `llama-server --list-devices` (clean environment)
  │
Step 4: Inspect GGUF model headers to confirm tensor quantization compatibility
  │
Step 5: Calculate VRAM offload sizing (Full fit > Partial offload > CPU)
  │
Step 6: Rank candidates; execute with automatic fallback cascade to CPU
```

---

## Vendor Arbitration Hierarchy

When multiple backends are present on a system, Tessera prioritizes candidates based on vendor runtime maturity:

| Vendor | Primary Candidate | Secondary Candidate | Fallback |
|---|---|---|---|
| **NVIDIA** | CUDA 12.4 | Vulkan | CPU |
| **AMD** | ROCm / HIP | Vulkan | CPU |
| **Intel** | SYCL | Vulkan (OpenVINO only if forced) | CPU |
| **Qualcomm** | OpenCL | Vulkan | CPU |
| **Other / Generic** | Vulkan | — | CPU |
| **Software Renderer** | *Never used* (WARP, llvmpipe skipped) | — | CPU |

### Multi-GPU Ranking
If a system has multiple physical GPUs (e.g., discrete RTX 4050 + integrated AMD/Intel graphics):
1. **Fit Entire Model:** A device that fits the entire model into VRAM beats partial offload.
2. **Discrete over Integrated:** Discrete GPUs take precedence over integrated GPUs even if the integrated GPU reports more shared system memory.
3. **Vendor Preference:** Vendor-native backends (CUDA/ROCm) take precedence over generic Vulkan.
4. **Free Memory:** Devices with greater unallocated VRAM take final priority.

---

## Manual Overrides

Developers can override the automated arbitration logic in `%LOCALAPPDATA%\Tessara\config.json`:

```json
{
  "inference": {
    "backend": "vulkan",
    "device": "0",
    "gpu_layers": 32,
    "allow_fallback": false
  }
}
```

- **`inference.backend`:** Forces a specific backend (`"cuda"`, `"vulkan"`, `"cpu"`).
- **`inference.device`:** Restricts execution to matching device index or name substring.
- **`inference.gpu_layers`:** Overrides automatic layer calculation.
- **`inference.allow_fallback`:** When `false`, halts with an error if the forced backend fails to start.
