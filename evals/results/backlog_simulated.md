# Backlog drain — SIMULATED costs, real scheduler

The scheduler is the production `SchedulerCore`; job costs are **assumptions** (Stage 0 targets), not measurements — no model has run on this machine. Criterion: everything from a 2-hour workload done ≤ 30 min after arrivals stop (default profile).

| profile | jobs | completed | failed | drained after end | max queue | preemptions | deferrals | retries | P0 wait p95 | model busy | verdict |
|---|---|---|---|---|---|---|---|---|---|---|---|
| gpu-4b (default) | 158 | 158 | 0 | 25 s | 7 | 0 | 2 | 0 | ≤ 100 ms | 12 % | drains |
| 2x arrivals (stress) | 354 | 354 | 0 | 25 s | 11 | 8 | 7 | 0 | ≤ 5000 ms | 26 % | drains |
| cpu-only (slow mode) | 158 | 158 | 0 | 157 s | 9 | 1 | 2 | 0 | ≤ 100 ms | 64 % | drains |

Assumptions per profile are in the JSON (`profile`). The CPU-only row is informational: the plan runs extraction on CPU-only machines only on AC power when idle.
