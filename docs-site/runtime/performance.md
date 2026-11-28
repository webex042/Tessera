---
title: Performance & Benchmarks
description: Empirical latency, memory, and hardware-profile measurements on AMD Radeon integrated graphics vs. discrete-GPU reference targets.
---

# Performance & Benchmarks

This page summarizes measured execution latencies and resource consumption on an **AMD Ryzen 5 7530U (AMD Radeon Integrated Graphics, Vulkan 1.3)** running `Qwen3-VL-4B-Instruct-Q4_K_M`, contrasted against both **Standard Product (`RTX 4050`)** targets and **Local iGPU Beta** targets.

---

## Latency & Throughput Benchmark Scorecard (`2026-11-28`)

| Workload / Benchmark | Standard Gate (`RTX 4050`) | Local iGPU Beta Gate | Measured (`AMD Radeon iGPU`) | Local Beta Status | Analysis |
|---|---|---|---|---|---|
| **Ask Time-to-First-Sentence (p50)** | $\le 2,500\text{ ms}$ | $\le 15,000\text{ ms}$ | **14,240 ± 516 ms** (`3×48` blind runs) | <StatusBadge status="pass" /> | Prefill reduced ~44% via `-b 2048 -ub 2048`, `MAX_SNIPPETS=3`, compact JSON |
| **Ask Full Generation (p50)** | $\le 10,000\text{ ms}$ | $\le 30,000\text{ ms}$ | **15,315 ± 490 ms** (`3×48` blind runs) | <StatusBadge status="pass" /> | Passes `local-igpu-beta`; exceeds discrete-GPU `standard` target |
| **Draft Time-to-First-Text (p50)** | $\le 3,000\text{ ms}$ | $\le 6,000\text{ ms}$ | **5,587 ms** (`20` scenarios) | <StatusBadge status="pass" /> | Passes `local-igpu-beta`; `mmproj` kept on CPU for Vulkan iGPU |
| **Draft Full Completion (p50)** | $\le 8,000\text{ ms}$ | $\le 20,000\text{ ms}$ | **14,175 ms** (`20` scenarios) | <StatusBadge status="pass" /> | Bounded `max_tokens=400` and capped thread/snippet windows |
| **UI Frame Budget (60 fps)** | $\le 16.6\text{ ms}$ | $\le 16.6\text{ ms}$ | **16.8 ms** p95 (`20` cycles) | <StatusBadge status="pass" /> | `2026-10-27` measurement (`notch_perf.json`) |
| **Notch Input-to-Visual Latency (p95)** | $\le 100\text{ ms}$ | $\le 100\text{ ms}$ | **64 ms** (`33 ms` p50) | <StatusBadge status="pass" /> | Cold-start cycle `64 ms`; subsequent cycles `24–39 ms` |

---

## Memory Footprint & Power Behavior

| Resource Metric | Context | Target Budget | Measured Observation |
|---|---|---|---|
| **GGUF Model Weights** | `Qwen3VL-4B-Instruct-Q4_K_M` + `mmproj-F16` | Stored in `models\` | `2.33 GiB` LLM + `0.78 GiB` mmproj (`tools/models.lock`) |
| **E5 Embedder Model** | `multilingual-e5-small-onnx` (CPU ONNX) | Avoids VRAM contention | `~465 MiB` total ONNX files (`onnxruntime.dll` `1.31.0`) |
| **Idle Worker Unload** | `inference.idle_unload_min = 10` | Free memory on idle | Sidecar stops cleanly and frees VRAM/shared RAM after inactivity |

::: info Hardware Disclosure
**RTX 4050 testing has not been performed.** All numbers above are specific to AMD Ryzen 5 7530U integrated graphics over shared system memory and should not be generalized across other GPUs. For full quality, extraction, security, and validation caveats, see [Measured Status and Limitations](/guide/limitations).
:::
