# ADR-0008: Stages 4–6 decisions, and guard changes from the first real-model run

Status: accepted, 2026-10-27. Evidence: `docs/stage4/`, `docs/stage5/`, `docs/stage6/`
verification reports, `docs/stage2/verification.md` (real-model rows).

## Stage 4 — rollups, brief, open loops

1. **Planning is derived, not queued.** Which periods need a rollup is computed from the store
   each minute (`tsr_pipeline::rollup::due`), so it is durable by construction; the `jobs` table
   is not used for rollups. Model rollups run as P2 scheduler jobs whose key carries the minute,
   because the scheduler never re-runs a completed key.
2. **Code-only (extractive) fallback.** Without a model, or after two invalid model outputs, a
   summary is written by code from item summaries/titles (hour) or child summaries (day and up),
   stored with `model = 'extractive'` and labelled in the UI. A model crash writes nothing (retry).
3. **Month = weeks overlapping the month** (plan: "week summaries"); a week that straddles two
   months feeds both. Drill-down month → week → day → hour → item works through
   `summary_sources`.
4. **Map-reduce** for inputs over ~3 000 tokens: each chunk summarised, then one call over the
   partial summaries writes the final text; highlights are taken by code from the validated partial
   highlights (so their ids stay verified).
5. **Rollup sentence rule.** Rollups may describe obligations ("Ravi needs the sheet by Friday") —
   the extraction summary rule (`obligation`) would reject those — but never instruct the reader
   (imperative openers, "you must…") or carry injected text.
6. **Staleness by trigger.** Deleting an item marks every summary whose period covers it stale
   (SQL trigger, all levels); writing or removing a summary marks its parents stale.
7. **Brief opener by code.** The plan's optional model-written opener is not used: the brief never
   waits for the model and contains no new model text (yesterday's summary was validated when it
   was written). "Waiting on" = the user's own open questions.
8. **Open loops: proposals before de-duplication.** Completion evidence ("sent the deck to Lena")
   is checked before dedupe, which would otherwise merge "sent X" into "send X" as plain evidence.
   The loop becomes `proposed_done` with the new evidence; only an approval-token command makes it
   `done`. If the loop names a person and the completion does not, that person must be mentioned
   in the completion's item (found by test: a completion addressed to Lena proposed Ravi's loop).
   A source-scan test fails if any other code path writes `done`/`dropped`.
9. **Bulk "dismiss all"** uses one token bound to the exact sorted set of fact ids.

## Stage 5 — drafts

1. **Streaming.** `ChatModel::chat_streaming` (default: one call with the whole reply) lets the
   loopback client pass deltas through; the draft text is decoded from the streaming JSON's
   `"draft"` string as it arrives.
2. **Claims moved to `tsr-guard::claims`** (shared with Ask; `tsr-draft` → `tsr-pipeline` →
   `tsr-retrieval` would otherwise be a dependency cycle).
3. **Clipboard: every memory format is saved and restored** (text, HTML, RTF, DIB, file lists,
   app formats); GDI-handle formats are skipped only when Windows re-synthesises them from a saved
   format; otherwise (or above 64 MB) Insert refuses and offers Copy. Text-only save would have
   refused almost every real clipboard (browsers and Office put HTML/RTF next to text).
4. **`ValuePattern.SetValue` fallback not built.** Insertion is clipboard + Ctrl+V into the
   re-verified field, or Copy. SetValue would need reading the field's current value.
5. **Target identity = window + UIA RuntimeId**, re-checked after focus returns; password state
   unknown counts as a password field.
6. Style samples (plan §12.1) are not collected yet; the prompt gets "(none yet)".

## Stage 6 — Ask

1. **No embedder in the app** until the owner approves one: retrieval is keyword + facts (RRF);
   the vector channel stays in the code and the evals.
2. **Past-oriented time filters** (`tsr_pipeline::askq`): "on Tuesday" means the most recent
   Tuesday (the due-date resolver points forward and is not reused).
3. **Citation chips open the item inside the notch**; nothing outside Tessara is opened, so the
   plan's `OpenSource` approval token is not needed yet (it is when opening the source app/URL).
4. Unverified sentences are shown dimmed with "not verified" (plan default).

## Guard changes from the real-model injection run (Stage 2 code, ADR-0006)

The real model copied whole source lines as fact text and evidence. Changes, each with a unit
test modelled on the real output:
- store-poisoning pattern: `:` after "memory" needs no word boundary (bug);
- "AI: run/execute/…" and destructive commands (`rm -rf /`, `drop table`, `curl … | sh`, …) are
  injections; "logs" is an exfiltration target;
- a code fence or a quoted (`>`) line anywhere in the evidence span makes it quoted;
- a summary is withheld when any fact in its item was an injection or quoted instruction, or
  when it repeats a quoted request.
Result: 66/74 → 70/74, all adversarial cases pass. Because these cases informed the fixes, the
corpus is no longer an unbiased held-out set; Stage 7 adds fresh cases written before any run.
