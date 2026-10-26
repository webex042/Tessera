-- 0001: blocklist with per-rule scope (plan §6.1, §9.2), settings, audit log.
-- Forward-only. Never edit after release; add a new numbered file instead.

CREATE TABLE blocklist (
  id       INTEGER PRIMARY KEY,
  key      TEXT    NOT NULL UNIQUE,          -- stable, content-free: 'builtin.title.otp', 'user.7'
  kind     TEXT    NOT NULL CHECK (kind IN ('process', 'title', 'url')),
  scope    TEXT    NOT NULL DEFAULT 'all' CHECK (scope IN ('all', 'browser')),
  pattern  TEXT    NOT NULL,
  is_regex INTEGER NOT NULL DEFAULT 0 CHECK (is_regex IN (0, 1)),
  builtin  INTEGER NOT NULL DEFAULT 0 CHECK (builtin IN (0, 1)),
  enabled  INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1))
);

CREATE TABLE settings (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL                        -- JSON
);

CREATE TABLE audit_log (
  id     INTEGER PRIMARY KEY,
  at     INTEGER NOT NULL,                   -- unix ms UTC
  event  TEXT    NOT NULL,
  detail TEXT                                -- content-free JSON (counts, ids, enum values)
);
CREATE INDEX audit_log_event_at ON audit_log(event, at);
