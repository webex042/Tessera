# Backlog drain — SIMULATED costs, real scheduler (profile: `standard`)

The scheduler is the production `SchedulerCore`; `gpu-4b` and `cpu-only` job costs are Stage 0 assumptions, while `igpu-4b` uses measured AMD Radeon iGPU latencies (~50 s extraction, 25.5 s Ask, 31.6 s rollup). Criterion: everything from a 2-hour workload done ≤ 30 min after arrivals stop.

| profile | jobs | completed | failed | drained after end | max queue | preemptions | deferrals | retries | P0 wait p95 | model busy | verdict |
|---|---|---|---|---|---|---|---|---|---|---|---|
| gpu-4b (default) | 158 | 158 | 0 | 25 s | 7 | 0 | 2 | 0 | ≤ 100 ms | 12 % | drains |
| 2x arrivals (stress) | 354 | 354 | 0 | 25 s | 11 | 8 | 7 | 0 | ≤ 5000 ms | 26 % | drains |
| igpu-4b (measured AMD iGPU) | 158 | 158 | 0 | 76 s | 9 | 2 | 15 | 1 | ≤ 100 ms | 66 % | drains |
| cpu-only (slow mode) | 158 | 158 | 0 | 157 s | 9 | 1 | 2 | 0 | ≤ 100 ms | 64 % | drains |

Assumptions per profile are in the JSON (`profile`). The CPU-only row is informational: the plan runs extraction on CPU-only machines only on AC power when idle.
