-- 0002: sources, items and full-text search (plan §9.2). Forward-only.

CREATE TABLE sources (
  id            INTEGER PRIMARY KEY,
  kind          TEXT NOT NULL CHECK (kind IN ('url', 'file', 'app')),
  app_exe       TEXT NOT NULL,
  app_name      TEXT,
  window_title  TEXT,                         -- redacted before storage
  url           TEXT,                         -- token-like query parameters and fragment removed
  file_path     TEXT,
  first_seen    INTEGER NOT NULL,             -- unix ms UTC
  last_seen     INTEGER NOT NULL
);
CREATE UNIQUE INDEX sources_key ON sources(kind, app_exe, IFNULL(url, ''), IFNULL(file_path, ''), IFNULL(window_title, ''));

CREATE TABLE items (
  id               INTEGER PRIMARY KEY,
  source_id        INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
  started_at       INTEGER NOT NULL,
  ended_at         INTEGER NOT NULL CHECK (ended_at >= started_at),
  acquisition      TEXT NOT NULL CHECK (acquisition IN ('uia', 'ocr', 'vlm', 'ext')),
  text             TEXT NOT NULL,             -- redacted
  summary          TEXT,                      -- from extraction (Stage 2)
  simhash          INTEGER NOT NULL,
  lang             TEXT,
  redaction_count  INTEGER NOT NULL DEFAULT 0,
  extraction       TEXT NOT NULL DEFAULT 'pending'
                   CHECK (extraction IN ('pending', 'done', 'failed', 'skipped'))
);
CREATE INDEX items_time ON items(started_at);
CREATE INDEX items_source ON items(source_id);

CREATE VIRTUAL TABLE items_fts USING fts5(
  text, summary, content = 'items', content_rowid = 'id',
  tokenize = 'unicode61 remove_diacritics 2'
);
-- Remove deleted terms from the index immediately instead of at the next merge, so a deleted
-- memory is not still searchable (or recoverable from index pages) afterwards.
INSERT INTO items_fts(items_fts, rank) VALUES ('secure-delete', 1);

CREATE TRIGGER items_ai AFTER INSERT ON items BEGIN
  INSERT INTO items_fts(rowid, text, summary) VALUES (new.id, new.text, new.summary);
END;
CREATE TRIGGER items_ad AFTER DELETE ON items BEGIN
  INSERT INTO items_fts(items_fts, rowid, text, summary) VALUES ('delete', old.id, old.text, old.summary);
END;
CREATE TRIGGER items_au AFTER UPDATE OF text, summary ON items BEGIN
  INSERT INTO items_fts(items_fts, rowid, text, summary) VALUES ('delete', old.id, old.text, old.summary);
  INSERT INTO items_fts(rowid, text, summary) VALUES (new.id, new.text, new.summary);
END;
