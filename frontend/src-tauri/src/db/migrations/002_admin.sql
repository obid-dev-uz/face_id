-- Administrator amallari jurnali: kim qachon kirdi, nima o'zgardi.
CREATE TABLE IF NOT EXISTS admin_events (
  id         INTEGER PRIMARY KEY,
  kind       TEXT NOT NULL,          -- 'unlock' | 'fail' | 'person_delete' ...
  detail     TEXT,
  created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_admin_events_id ON admin_events(id DESC);
