CREATE TABLE memory_captures (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  session_id TEXT NOT NULL,
  folder_path TEXT NOT NULL,
  title TEXT,
  bank TEXT NOT NULL,
  trigger TEXT NOT NULL,
  anchor_entry_id TEXT,
  started_at TEXT NOT NULL,
  ended_at TEXT,
  error TEXT,
  no_model INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX memory_captures_session ON memory_captures (session_id, id);
CREATE TABLE memory_capture_facts (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  capture_id INTEGER NOT NULL REFERENCES memory_captures(id) ON DELETE CASCADE,
  outcome TEXT NOT NULL,
  category TEXT NOT NULL,
  text TEXT NOT NULL,
  previous TEXT,
  note TEXT NOT NULL,
  path TEXT NOT NULL,
  line_text TEXT,
  previous_path TEXT,
  previous_before TEXT,
  previous_after TEXT,
  reverted INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX memory_capture_facts_capture ON memory_capture_facts (capture_id);
