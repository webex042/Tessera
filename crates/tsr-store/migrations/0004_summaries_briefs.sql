-- 0004: rollups with provenance, morning briefs (plan §9.2, §11).
-- Forward-only. Rollback guidance: restore the pre-upgrade backup; nothing here rewrites rows of
-- earlier tables.

CREATE TABLE summaries (
  id            INTEGER PRIMARY KEY,
  level         TEXT NOT NULL CHECK (level IN ('hour', 'day', 'week', 'month')),
  period_start  INTEGER NOT NULL,                 -- local-time boundary stored as UTC ms
  period_end    INTEGER NOT NULL CHECK (period_end > period_start),
  text          TEXT NOT NULL CHECK (length(text) <= 2000),
  highlights    TEXT NOT NULL CHECK (json_valid(highlights)),   -- [{text, item_ids[], child_ids[]}]
  stale         INTEGER NOT NULL DEFAULT 0 CHECK (stale IN (0, 1)),
  prompt_ver    TEXT NOT NULL,
  model         TEXT NOT NULL,                    -- model id, or 'extractive' (code-only fallback)
  created_at    INTEGER NOT NULL,
  UNIQUE (level, period_start)
);
CREATE INDEX summaries_period ON summaries(level, period_start, period_end);
CREATE INDEX summaries_stale ON summaries(stale) WHERE stale = 1;

-- Provenance: each summary lists the items (hour level) or child summaries (day and up) it was
-- made from, so a month highlight drills down to the original items.
CREATE TABLE summary_sources (
  summary_id  INTEGER NOT NULL REFERENCES summaries(id) ON DELETE CASCADE,
  item_id     INTEGER REFERENCES items(id) ON DELETE CASCADE,
  child_id    INTEGER REFERENCES summaries(id) ON DELETE CASCADE,
  CHECK ((item_id IS NULL) <> (child_id IS NULL))
);
CREATE INDEX summary_sources_by_summary ON summary_sources(summary_id);
CREATE INDEX summary_sources_by_item ON summary_sources(item_id);
CREATE INDEX summary_sources_by_child ON summary_sources(child_id);

-- Deleting an item marks every summary whose period covers it stale, at every level, so no
-- summary keeps describing deleted content; the rollup worker regenerates them (plan §11.1).
CREATE TRIGGER items_delete_marks_summaries_stale BEFORE DELETE ON items BEGIN
  UPDATE summaries SET stale = 1
   WHERE period_start <= OLD.started_at AND OLD.started_at < period_end;
END;

CREATE TABLE briefs (
  day         TEXT PRIMARY KEY CHECK (length(day) = 10),   -- local date, YYYY-MM-DD
  json        TEXT NOT NULL CHECK (json_valid(json)),
  created_at  INTEGER NOT NULL
);
