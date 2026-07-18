pub const SCHEMA_VERSION: i64 = 1;

pub const MIGRATION_1: &str = r#"
CREATE TABLE IF NOT EXISTS meta (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS notes (
  scope TEXT NOT NULL,
  stable_key TEXT NOT NULL,
  note TEXT NOT NULL,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY (scope, stable_key)
);

CREATE TABLE IF NOT EXISTS live_targets (
  target_kind TEXT NOT NULL,
  tmux_id TEXT NOT NULL,
  stable_key TEXT NOT NULL,
  name TEXT,
  cwd TEXT,
  session_id TEXT,
  window_id TEXT,
  pane_id TEXT,
  last_seen_at INTEGER NOT NULL,
  PRIMARY KEY (target_kind, tmux_id)
);

CREATE TABLE IF NOT EXISTS mru_targets (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  target_kind TEXT NOT NULL,
  stable_key TEXT NOT NULL,
  tmux_target TEXT,
  client_tty TEXT,
  visited_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_mru_targets_visited_at ON mru_targets(visited_at DESC);
CREATE INDEX IF NOT EXISTS idx_mru_targets_target ON mru_targets(target_kind, tmux_target, stable_key);

CREATE TABLE IF NOT EXISTS projects (
  path TEXT PRIMARY KEY,
  repo_root TEXT,
  name TEXT NOT NULL,
  source TEXT NOT NULL,
  last_used_at INTEGER,
  frecency_score REAL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS layouts (
  name TEXT PRIMARY KEY,
  path TEXT NOT NULL,
  source TEXT NOT NULL,
  trusted INTEGER NOT NULL DEFAULT 0,
  content_hash TEXT,
  last_used_at INTEGER
);
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn migration_applies() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(MIGRATION_1).unwrap();
        conn.execute(
            "INSERT INTO meta(key, value) VALUES('schema_version', ?1)",
            [SCHEMA_VERSION.to_string()],
        )
        .unwrap();
        let v: String = conn
            .query_row(
                "SELECT value FROM meta WHERE key='schema_version'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(v, "1");
    }
}
