CREATE TABLE IF NOT EXISTS character_snapshots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    character_key TEXT NOT NULL,
    captured_at TEXT NOT NULL,
    schema_version INTEGER NOT NULL,
    snapshot_json TEXT NOT NULL,
    UNIQUE(character_key, captured_at)
);

CREATE INDEX IF NOT EXISTS idx_character_snapshots_history
    ON character_snapshots(character_key, captured_at DESC);
