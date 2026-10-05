CREATE TABLE IF NOT EXISTS persons (
  id          INTEGER PRIMARY KEY,
  uuid        TEXT NOT NULL UNIQUE,
  full_name   TEXT NOT NULL,
  external_id TEXT,
  note        TEXT,
  avatar      BLOB,
  consent_at  TEXT,
  is_active   INTEGER NOT NULL DEFAULT 1,
  created_at  TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE IF NOT EXISTS face_embeddings (
  id         INTEGER PRIMARY KEY,
  person_id  INTEGER NOT NULL REFERENCES persons(id) ON DELETE CASCADE,
  model      TEXT NOT NULL,          -- 'sface-2021dec'
  dim        INTEGER NOT NULL,
  vector     BLOB NOT NULL,          -- f32 LE, L2-normallangan
  quality    REAL,
  thumb      BLOB,
  device     TEXT,                   -- 'android-24' | 'windows-7' ...
  created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_emb_person ON face_embeddings(person_id);
CREATE TABLE IF NOT EXISTS recognition_logs (
  id         INTEGER PRIMARY KEY,
  person_id  INTEGER REFERENCES persons(id) ON DELETE SET NULL,
  score      REAL NOT NULL,
  matched    INTEGER NOT NULL,
  source     TEXT NOT NULL,
  snapshot   BLOB,
  created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_logs_time ON recognition_logs(created_at);
CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
