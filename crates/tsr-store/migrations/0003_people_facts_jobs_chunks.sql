-- 0003: people, suppression, facts with evidence, durable jobs, chunks (plan §8, §9.2, §9.4).
-- Forward-only. Rollback guidance: restore the pre-upgrade backup (the app copies tessara.db
-- before migrating); nothing here rewrites rows of earlier tables.

CREATE TABLE people (
  id            INTEGER PRIMARY KEY,
  display_name  TEXT NOT NULL CHECK (length(display_name) BETWEEN 1 AND 80),
  is_self       INTEGER NOT NULL DEFAULT 0 CHECK (is_self IN (0, 1)),
  created_at    INTEGER NOT NULL
);
CREATE UNIQUE INDEX people_one_self ON people(is_self) WHERE is_self = 1;

-- Aliases are stored normalised (tsr-pipeline::people::normalize_alias).
-- Deviation from the plan's UNIQUE(kind, alias): two different people may share a *name*, so
-- only e-mail addresses and handles are unique across people.
CREATE TABLE person_aliases (
  person_id  INTEGER NOT NULL REFERENCES people(id) ON DELETE CASCADE,
  kind       TEXT NOT NULL CHECK (kind IN ('name', 'email', 'handle')),
  alias      TEXT NOT NULL CHECK (length(alias) BETWEEN 1 AND 120),
  UNIQUE (person_id, kind, alias)
);
CREATE INDEX person_aliases_lookup ON person_aliases(kind, alias);
CREATE UNIQUE INDEX person_aliases_ids ON person_aliases(kind, alias) WHERE kind IN ('email', 'handle');

-- Possible duplicates for the user to confirm; never merged automatically (plan §8.5).
CREATE TABLE person_merge_suggestions (
  a           INTEGER NOT NULL REFERENCES people(id) ON DELETE CASCADE,
  b           INTEGER NOT NULL REFERENCES people(id) ON DELETE CASCADE,
  reason      TEXT NOT NULL CHECK (reason IN ('same_name', 'first_name_and_domain', 'name_prefix')),
  created_at  INTEGER NOT NULL,
  PRIMARY KEY (a, b),
  CHECK (a < b)
);

-- Forgotten identities (plan §15.5). Only a keyed SHA-256 of the normalised alias is kept, so
-- the table cannot be read back into names; the key is a random per-database salt.
CREATE TABLE suppressed_aliases (
  kind        TEXT NOT NULL CHECK (kind IN ('name', 'email', 'handle')),
  digest      TEXT NOT NULL CHECK (length(digest) = 64),
  created_at  INTEGER NOT NULL,
  PRIMARY KEY (kind, digest)
);
INSERT INTO settings(key, value) VALUES ('suppression_salt', json_quote(lower(hex(randomblob(32))))) ON CONFLICT DO NOTHING;

CREATE TABLE facts (
  id                 INTEGER PRIMARY KEY,
  kind               TEXT NOT NULL CHECK (kind IN ('task', 'commitment', 'decision', 'event', 'question', 'note')),
  text               TEXT NOT NULL CHECK (length(text) BETWEEN 1 AND 200),
  text_norm          TEXT NOT NULL,                -- normalised words, for de-duplication
  owner_id           INTEGER REFERENCES people(id) ON DELETE SET NULL,
  owner_text         TEXT CHECK (owner_text IS NULL OR length(owner_text) BETWEEN 1 AND 80),
  counterparty_id    INTEGER REFERENCES people(id) ON DELETE SET NULL,
  counterparty_text  TEXT CHECK (counterparty_text IS NULL OR length(counterparty_text) BETWEEN 1 AND 80),
  due_text           TEXT CHECK (due_text IS NULL OR length(due_text) BETWEEN 1 AND 60),
  due_at             INTEGER,
  due_precision      TEXT CHECK (due_precision IS NULL OR due_precision IN ('datetime', 'date', 'week', 'month')),
  due_ambiguous      INTEGER NOT NULL DEFAULT 0 CHECK (due_ambiguous IN (0, 1)),
  status             TEXT NOT NULL DEFAULT 'open'
                     CHECK (status IN ('open', 'done', 'dropped', 'proposed_done', 'needs_review')),
  confidence         TEXT NOT NULL CHECK (confidence IN ('low', 'medium', 'high')),
  prompt_ver         TEXT NOT NULL,
  model              TEXT NOT NULL,
  schema_ver         INTEGER NOT NULL,
  redactions         INTEGER NOT NULL DEFAULT 0 CHECK (redactions >= 0),
  redact_ver         TEXT NOT NULL,
  first_seen         INTEGER NOT NULL,
  last_seen          INTEGER NOT NULL,
  merged_count       INTEGER NOT NULL DEFAULT 0 CHECK (merged_count >= 0),
  CHECK ((due_at IS NULL) = (due_precision IS NULL)),
  CHECK (due_ambiguous = 0 OR due_at IS NOT NULL),
  CHECK (last_seen >= first_seen)
);
CREATE INDEX facts_open ON facts(status, kind, due_at);
CREATE INDEX facts_dedupe ON facts(kind, owner_id, last_seen);

-- Every fact has at least one evidence row: a quote and its character span in an item.
CREATE TABLE fact_evidence (
  fact_id   INTEGER NOT NULL REFERENCES facts(id) ON DELETE CASCADE,
  item_id   INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
  quote     TEXT NOT NULL CHECK (length(quote) BETWEEN 1 AND 200),
  start_ch  INTEGER NOT NULL CHECK (start_ch >= 0),
  end_ch    INTEGER NOT NULL,
  match     TEXT NOT NULL CHECK (match IN ('exact', 'normalized', 'fuzzy')),
  added_at  INTEGER NOT NULL,
  PRIMARY KEY (fact_id, item_id, start_ch),
  CHECK (end_ch > start_ch)
);
CREATE INDEX fact_evidence_item ON fact_evidence(item_id);

-- A fact whose last evidence is gone (item deleted, evidence invalidated) is not a fact any more.
CREATE TRIGGER fact_evidence_last_ad AFTER DELETE ON fact_evidence
WHEN NOT EXISTS (SELECT 1 FROM fact_evidence WHERE fact_id = old.fact_id)
BEGIN
  DELETE FROM facts WHERE id = old.fact_id;
END;

CREATE VIRTUAL TABLE facts_fts USING fts5(
  text, content = 'facts', content_rowid = 'id',
  tokenize = 'unicode61 remove_diacritics 2'
);
INSERT INTO facts_fts(facts_fts, rank) VALUES ('secure-delete', 1);
CREATE TRIGGER facts_ai AFTER INSERT ON facts BEGIN
  INSERT INTO facts_fts(rowid, text) VALUES (new.id, new.text);
END;
CREATE TRIGGER facts_ad AFTER DELETE ON facts BEGIN
  INSERT INTO facts_fts(facts_fts, rowid, text) VALUES ('delete', old.id, old.text);
END;
CREATE TRIGGER facts_au AFTER UPDATE OF text ON facts BEGIN
  INSERT INTO facts_fts(facts_fts, rowid, text) VALUES ('delete', old.id, old.text);
  INSERT INTO facts_fts(rowid, text) VALUES (new.id, new.text);
END;

-- Durable job state (plan §8.1 steps 4–5). The scheduler is in memory; this table is what
-- survives a restart: queued and running jobs are re-queued on start.
CREATE TABLE jobs (
  id          INTEGER PRIMARY KEY,
  kind        TEXT NOT NULL CHECK (kind IN ('extract', 'embed', 'vlm')),
  item_id     INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
  state       TEXT NOT NULL CHECK (state IN ('queued', 'running', 'done', 'failed', 'skipped', 'cancelled')),
  attempts    INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
  stage       TEXT,           -- last stage reached; content-free label
  error       TEXT,           -- content-free error code
  created_at  INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL,
  UNIQUE (kind, item_id)
);
CREATE INDEX jobs_state ON jobs(state, kind);

-- Embedding chunks (plan §9.4): int8 vectors with one f32 scale, versioned by model and chunker.
CREATE TABLE chunks (
  id           INTEGER PRIMARY KEY,
  item_id      INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
  ord          INTEGER NOT NULL CHECK (ord >= 0),
  start_ch     INTEGER NOT NULL CHECK (start_ch >= 0),
  end_ch       INTEGER NOT NULL,
  chunker_ver  TEXT NOT NULL,
  model        TEXT NOT NULL,
  dims         INTEGER NOT NULL CHECK (dims > 0),
  scale        REAL NOT NULL CHECK (scale > 0),
  vec          BLOB NOT NULL,
  UNIQUE (item_id, model, ord),
  CHECK (end_ch > start_ch),
  CHECK (length(vec) = dims)
);
CREATE INDEX chunks_model ON chunks(model);
