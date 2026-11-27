# Hardware-Aware Evaluation Scorecard — Standard Product vs. Local iGPU Beta

- **Profile 1:** Standard Product Profile (RTX 4050 / Discrete GPU Target)
- **Profile 2:** Local iGPU Beta Profile (Qwen3-VL-4B Q4_K_M on AMD Radeon iGPU)
- **Measured Hardware Baseline:** AMD Ryzen 5 7530U (AMD Radeon Integrated Graphics via Vulkan), Windows 11, `Qwen3-VL-4B-Instruct-Q4_K_M`
- **Audit Rule:** Measured results are preserved untouched. Provisional thresholds (`>= 65%` Ask, `>= 0.50` Extraction F1) were **rejected** after variance/sample analysis; security thresholds remain 100% strict.

| Metric | Measured Result | Standard Target | Local iGPU Target | Local Status |
|---|---|---|---|---|
| Ask accuracy (60 questions) | 66.7 % (40/60) | ≥ 80 % (blind) | ≥ 70 % (blind; ≥65% rejected) | FAIL |
| Ask citation verification / fabricated | 83.7 % verified; 0 fabricated | ≥ 90 % verified; 0 fabricated | ≥ 85 % verified; 0 fabricated | FAIL (0 fabricated PASS) |
| Ask latency (p50 first / total) | 25155 ms / 25540 ms | ≤ 2500 ms / ≤ 10000 ms | ≤ 15000 ms / ≤ 30000 ms | FAIL |
| Extraction F1 (58 held-out items) | 0.533 (P 49.5 %, R 57.8 %) | ≥ 0.70 | ≥ 0.60 (≥0.50 rejected) | FAIL |
| Extraction safety (hallucinated / invalid) | 0.0 % hallucinated; 0 invalid | ≤ 3 % hallucinated; 0 invalid | ≤ 3 % hallucinated; 0 invalid | PASS |
| Prompt injection resistance (real 4B corpus) | 123/152 safe (24/39 held-out; 0 sim fails) | 152/152 safe (0 failures) | 152/152 safe (0 failures; non-negotiable) | FAIL (historical real run; code guard 152/152 sim + 13/13 replay) |
| Secret redaction (355 secrets / 242 clean) | 355/355 (100.0 %), 0.41 % FP | 100 % redacted, ≤ 2 % FP | 100 % redacted, ≤ 2 % FP (non-negotiable) | PASS |
| Notch input-to-visual latency (p95) | 64 ms (frame p95 16.8 ms) | ≤ 100 ms | ≤ 100 ms | PASS |
| Screen capture exclusion | 100 % WGC/GDI pixel pass; named apps untested | Absent in all (incl. Teams/Zoom/OBS) | Absent in all (incl. Teams/Zoom/OBS) | PARTIAL |
| Draft safety & latency (20 scenarios) | 17/20 valid, 0 unflagged, 5870 ms first | all valid, 0 unflagged, ≤ 3000 ms | all valid, 0 unflagged, ≤ 6000 ms | PARTIAL / FAIL (17/20 valid) |
| Rollups (real 4B week replay) | 11/11 active hours, 0 % fallback, 0 invalid ids | exact hours, ≤ 10 % fallback, 0 invalid ids | exact hours, ≤ 10 % fallback, 0 invalid ids | PASS |
| Search behaviour checks (simulated embedder) | 6/6 checks pass; recall@10 = 1.000 (sim hash) | All behaviour checks pass + E5 eval | All behaviour checks pass (E5 unmeasured) | PASS (behaviour) / PARTIAL (E5 unmeasured) |
| Outbound network (application processes) | 0 app connections (tessara.exe, llama-server.exe) | 0 unauthorized connections | 0 unauthorized connections (non-negotiable) | PASS (10-min; 24h Sysmon/WFP pending) |
| Outbound network (WebView2 runtime, ADR-0002) | 1 HTTPS connection to Microsoft 2603:1046:c04::/48:443 | 0 connections (or verified disabled) | 0 connections (or documented SNI + blocked) | FAIL (open ADR-0002) |

- **Backlog Drain Simulation (measured iGPU job latencies):** `76 s` after 2-hour arrival window (threshold `<= 1800 s`: **PASS**).
