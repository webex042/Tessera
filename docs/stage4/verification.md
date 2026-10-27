# Stage 4 verification report: rollups, morning brief, open loops

Date: 2026-10-27. Same machine as Stage 3. Decisions: [ADR-0008](../adr/0008-stages-4-6-and-real-model-guard.md).

## A. What was built

| task | where |
|---|---|
| Hour/day/week/month rollups with provenance, staleness, regeneration, map-reduce, extractive fallback | migration `0004_summaries_briefs.sql`; `tsr-store::summaries`; `tsr-pipeline::rollup`; `RollupRunner` (P2) |
| Brief assembly + trigger (first unlock or start after 05:00) | `tsr-pipeline::brief`; `Services::brief_if_due`; `brief_ready` peek |
| Open loops: groups, proposed-done flow, "Still open?" with dismiss-all | `tsr-pipeline::loops`; extraction hook; `Core::{loop_groups, propose_bulk_status, set_bulk_status}` |
| Timeline Day / Week / Month with drill-down | `Core::{rollups, items_by_ids}`; `TimelineView`, `Rollup.tsx` |
| Today: brief opener, yesterday, "May be done" | `TodayView` |

## B. Exit criteria

| criterion | measured | evidence | status |
|---|---|---|---|
| Every highlight's ids exist and resolve to items; drill-down month → item | every highlight at hour/day/week level cites existing items; invalid ids (999 999) stripped; a highlight citing only invalid ids dropped; week and month `source_items` equal exactly the replayed items | `crates/tsr-pipeline/tests/rollup.rs` (7-day replay, month test, deletion test); `src-tauri/tests/views.rs::week_and_month_views_drill_down_to_items`; UI drill-down seen in preview | **PASS** |
| Brief ready ≤ 30 s after first unlock | REAL app start-up trigger on a synthetic store: **7 ms**; 3 000 facts: **42 ms**; the unlock trigger calls the same function on `WTS_SESSION_UNLOCK` | frame log `brief_ms`; `loops_brief.rs::brief_on_a_large_store…` | **PARTIAL**: timing PASS; the unlock path was not exercised live (locking the session is the owner's action) |
| 7-day replay: rollups for every active hour, none for idle hours | 26 active hours → exactly 26 hour summaries; 6 active days → 6 day summaries (idle Sunday none); 1 week; no month before October ends | `rollup.rs::seven_day_replay…` (SIMULATED model) and same with code-only rollups | **PASS** (SIMULATED model); real-model rollup run: see §C |
| No fact auto-closed without user confirmation | completion evidence → `proposed_done`, never `done`; a source scan fails if any code path other than the approval-token command writes `done`/`dropped`; plans ("will send") and other people's completions do not propose | `loops_brief.rs`, `no_auto_close.rs` | **PASS** |

## C. Real model

Rollups through the real model (`tsr-eval rollup --model DIR`): running at time of writing; see
`evals/results/rollup_real.md` when present.

## D. Findings fixed during the stage

- A completion addressed to Lena proposed Ravi's open loop as done (the validator had dropped
  Lena's full name as counterparty). Fix: the loop's person must be mentioned in the completion's item.
- "sent the revised sheet" de-duplicated into "send the revised sheet" (Dice 0.86) as plain
  evidence, so no proposal would ever appear. Fix: completion check runs before de-duplication.
- Week label in the Timeline header used the selected day, not the week's Monday.
