# ADR-0010: Keep JSON Schema key order when sending schemas to the model

Status: accepted, 2026-10-28. Evidence: `evals/results/ask_real_v2_sortedkeys*`,
`evals/results/ask_real_all*`, `crates/tsr-infer/src/prompts.rs`
(`schemas_keep_their_declared_key_order`).

## Context

llama-server turns the `response_format` schema into a grammar that makes the model write object
keys in the order it receives them. The workspace built `serde_json` without `preserve_order`, so
every schema was parsed into a sorted map and sent with its keys in alphabetical order. The order
written in the schema files was never the order the model saw:

| prompt | declared | sent |
|---|---|---|
| ask v1/v2 | `sentences`, `insufficient`; sentence `text`, `cites` | `insufficient` first; `cites` before `text` |
| extraction v1 | `summary`, `facts`, `people`; fact `kind`, `text`, …, `confidence` | `facts` first; fact `confidence` first, `text` last |
| rollup v1 | `summary`, `highlights`; `text`, `ids` | `highlights` first; `ids` before `text` |
| draft v1 | `draft`, `used`, `placeholders` | `placeholders` before `used` |

Recorded raw outputs showed the effect on Ask: with `insufficient` forced first, the model wrote
`"insufficient": false` and then an empty list (5 / 5 diagnosis questions; 48 / 60 in the full
ask/v2 run).

## Decision

Enable `serde_json`'s `preserve_order` feature for the whole workspace, so the app and the eval
runner send each schema in its declared order. A unit test fails if a schema is reordered again.
The prompt and schema files themselves are unchanged, so prompt versions stay the same; results
measured before this change are kept and labelled "sorted keys".

## Consequences

- Every real-model result recorded before 2026-10-28 11:20 IST (extraction, injection, rollups,
  drafts, Ask) was measured with sorted keys. Ask and held-out extraction were both re-run in
  full; drafts and rollups are still only re-checked on subsets.
- Held-out extraction re-run (2026-10-28, ~50 min, not ~6 h — the invalid-JSON retries are gone):
  F1 0.507 → 0.533, still FAIL; the fix removed all 15/58 invalid-JSON failures (recall
  44.6 % → 57.8 %) but precision fell (58.7 % → 49.5 %) because the model stores more low-value
  `note` facts. So key order was one cause of the low score, not the whole story.
- No new dependency is downloaded: `indexmap` 2.14.2 and `foldhash` were already in the lock
  file (the lock gains two edges from `serde_json` to them).
