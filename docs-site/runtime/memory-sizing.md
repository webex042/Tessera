# Memory Sizing & VRAM

This document breaks down the mathematical memory requirements for running Tessera's default vision-language model (`Qwen3-VL-4B-Instruct-Q4_K_M`) within consumer VRAM budgets.

---

## VRAM Allocation Budget (8,192 Context Window)

| Component | Format / Precision | Calculation & Sizing | Memory Footprint |
|---|---|---|---|
| **Model Weights** | Q4_K_M Quantized | $\sim 4.2\times 10^9 \text{ parameters} \times \approx 4.5 \text{ bits/param}$ | $\approx 2.45\text{ GB}$ |
| **Vision Projector (`mmproj`)** | F16 Precision | Visual encoder weights + projection layers | $\approx 0.80\text{ GB}$ |
| **KV Cache (8k Tokens)** | Q8_0 Quantized | $2 \times 36 \text{ layers} \times 8 \text{ KV heads} \times 128 \text{ dim} \times 8,192 \text{ ctx} \times 1 \text{ byte}$ | $\approx 0.60\text{ GB}$ |
| **Compute Buffers & Graph** | Mixed F16/F32 | Scratch tensors, attention matrices, downsampled image tokens | $\approx 0.65\text{ GB} - 1.15\text{ GB}$ |
| **Total VRAM Footprint** | | Baseline budget for full layer offload | **$\approx 4.50\text{ GB} - 5.00\text{ GB}$** |

::: tip 6 GB Reference Fit
The calculated total allocation ($\approx 4.5 - 5.0\text{ GB}$) comfortably fits within the 5.2 GB allocation gate of the standard **6 GB reference GPU** (NVIDIA RTX 4050 laptop class), leaving $\ge 1.0\text{ GB}$ of VRAM for desktop compositing and OS displays.
:::

---

## Memory Degradation & Fallback Sizing

When available free VRAM falls below $5.2\text{ GB}$, Tessera applies tiered degradation heuristics:

1. **Vision Projector Offload (`--no-mmproj-offload`):**
   - Moves the 0.80 GB vision encoder to system RAM.
   - Saves 800 MB of VRAM with minimal impact on text generation latency.
2. **Partial Layer Offload:**
   - Offloads $N$ layers to GPU until VRAM reaches $85\%$ capacity, executing remaining layers on CPU.
   - Permitted only if at least $25\%$ of layers fit on the GPU.
3. **Full CPU Fallback:**
   - If fewer than $25\%$ of layers fit or driver allocation fails, all execution reverts to the CPU.
