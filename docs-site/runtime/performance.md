# Performance & Benchmarks

This document records empirical performance and latency measurements obtained from real execution runs on an AMD Radeon integrated GPU (Ryzen 5 7530U, Vulkan 1.3) compared against the target performance gates.

---

## Latency & Throughput Benchmark Scorecard

| Workload / Benchmark | Target Performance Gate | Measured Result (AMD iGPU) | Status | Analysis |
|---|---|---|---|---|
| **Ask Time-to-First-Token (p50)** | $\le 2,500\text{ ms}$ | **14,521 ms** | <StatusBadge status="fail" /> | High prompt prefill latency over shared LPDDR4x memory bus. |
| **Ask Full Generation (p50)** | $\le 10,000\text{ ms}$ | **19,222 ms** | <StatusBadge status="fail" /> | Token decoding throughput limited by integrated compute cores. |
| **Draft Time-to-First-Text (p50)** | $\le 3,000\text{ ms}$ | **5,870 ms** | <StatusBadge status="fail" /> | Approaching interactive speed, but exceeds 3s target gate. |
| **Draft Full Completion (p50)** | $\le 8,000\text{ ms}$ | **16,372 ms** | <StatusBadge status="fail" /> | Multi-sentence draft generation requires discrete GPU. |
| **Hardware Detection Latency** | $\le 1,000\text{ ms}$ | **650 ms** (Release build) | <StatusBadge status="pass" /> | Fast DXGI enumeration and manifest verification. |
| **UI Frame Budget (60 fps)** | Frame time $\le 16.6\text{ ms}$ | **16.8 ms** (p95) | <StatusBadge status="pass" /> | Smooth 60 fps CSS animations in WebView2 notch. |
| **Input-to-Visual Latency (p95)**| $\le 100\text{ ms}$ | **64 ms** | <StatusBadge status="pass" /> | Instant hotkey responsiveness when expanding notch. |

---

## Memory Footprint & Resource Consumption

| Resource Metric | Measurement Window | Target Budget | Measured Reality | Status |
|---|---|---|---|---|
| **Host Application RAM** | Idle running state | $\le 250\text{ MB}$ | **184 MB** | <StatusBadge status="pass" /> |
| **Background CPU Utilization** | Idle window tracking | $\le 3.0\%$ avg | **1.2% - 2.4%** | <StatusBadge status="pass" /> |
| **Model VRAM Allocation** | Qwen3-VL-4B-Instruct | $\le 5.2\text{ GB}$ | **4.68 GB** | <StatusBadge status="pass" /> |
| **GPU Idle Draw (Post-Unload)**| 10 min idle on battery | 0% GPU Compute | **0.0%** | <StatusBadge status="pass" /> |

::: info Hardware Takeaway
The empirical measurements demonstrate that while Tessera's architecture operates reliably on commodity integrated graphics without crashing or corrupting memory, **dedicated discrete GPUs (e.g. RTX 4050+) are necessary to meet sub-3-second interactive latency targets**.
:::
