pub mod schema;

use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::{Path, PathBuf};

use crate::config::ensure_parent_dir;
use crate::tmux::formats::{PaneInfo, SessionInfo, WindowInfo};

#[derive(Debug)]
pub struct Store {
    conn: Connection,
    path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct MruEntry {
    pub target_kind: String,
    pub stable_key: String,
    pub tmux_target: Option<String>,
    pub client_tty: Option<String>,
    pub visited_at: i64,
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        ensure_parent_dir(&path)?;
        let conn = Connection::open(&path)
            .with_context(|| format!("opening state DB {}", path.display()))?;
        let store = Self { conn, path };
        store.migrate()?;
        Ok(store)
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let store = Self {
            conn,
            path: PathBuf::from(":memory:"),
        };
        store.migrate()?;
        Ok(store)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn migrate(&self) -> Result<()> {
        self.conn
            .execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")
            .ok();
        self.conn.execute_batch(schema::MIGRATION_1)?;
        self.conn.execute(
            "INSERT INTO meta(key, value) VALUES('schema_version', ?1)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [schema::SCHEMA_VERSION.to_string()],
        )?;
        Ok(())
    }

    pub fn set_note(&self, scope: &str, stable_key: &str, note: &str) -> Result<i64> {
        let ts = Utc::now().timestamp();
        self.conn.execute(
            "INSERT INTO notes(scope, stable_key, note, updated_at) VALUES(?1, ?2, ?3, ?4)
             ON CONFLICT(scope, stable_key) DO UPDATE SET note=excluded.note, updated_at=excluded.updated_at",
            params![scope, stable_key, note, ts],
        )?;
        Ok(ts)
    }

    pub fn get_note(&self, scope: &str, stable_key: &str) -> Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT note FROM notes WHERE scope=?1 AND stable_key=?2",
                params![scope, stable_key],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn push_mru(
        &self,
        target_kind: &str,
        stable_key: &str,
        tmux_target: Option<&str>,
        client_tty: Option<&str>,
    ) -> Result<i64> {
        let ts = Utc::now().timestamp();
        self.conn.execute(
            "INSERT INTO mru_targets(target_kind, stable_key, tmux_target, client_tty, visited_at) VALUES(?1, ?2, ?3, ?4, ?5)",
            params![target_kind, stable_key, tmux_target, client_tty, ts],
        )?;
        self.prune_mru(1000)?;
        Ok(ts)
    }

    pub fn recent_mru(&self, limit: usize) -> Result<Vec<MruEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT target_kind, stable_key, tmux_target, client_tty, visited_at
             FROM (
               SELECT *, ROW_NUMBER() OVER (
                 PARTITION BY target_kind, COALESCE(tmux_target, stable_key)
                 ORDER BY visited_at DESC, id DESC
               ) AS rn
               FROM mru_targets
             )
             WHERE rn = 1
             ORDER BY visited_at DESC, id DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map([limit as i64], |row| {
            Ok(MruEntry {
                target_kind: row.get(0)?,
                stable_key: row.get(1)?,
                tmux_target: row.get(2)?,
                client_tty: row.get(3)?,
                visited_at: row.get(4)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn prune_mru(&self, keep: usize) -> Result<()> {
        self.conn.execute(
            "DELETE FROM mru_targets WHERE id NOT IN (SELECT id FROM mru_targets ORDER BY visited_at DESC, id DESC LIMIT ?1)",
            [keep as i64],
        )?;
        Ok(())
    }

    pub fn upsert_project(
        &self,
        path: &str,
        repo_root: Option<&str>,
        name: &str,
        source: &str,
    ) -> Result<()> {
        let ts = Utc::now().timestamp();
        self.conn.execute(
            "INSERT INTO projects(path, repo_root, name, source, last_used_at, frecency_score) VALUES(?1, ?2, ?3, ?4, ?5, 1)
             ON CONFLICT(path) DO UPDATE SET repo_root=excluded.repo_root, name=excluded.name, source=excluded.source, last_used_at=excluded.last_used_at, frecency_score=projects.frecency_score + 1",
            params![path, repo_root, name, source, ts],
        )?;
        Ok(())
    }

    pub fn sync_live_targets(
        &self,
        sessions: &[SessionInfo],
        windows: &[WindowInfo],
        panes: &[PaneInfo],
    ) -> Result<()> {
        let ts = Utc::now().timestamp();
        for s in sessions {
            self.conn.execute(
                "INSERT INTO live_targets(target_kind, tmux_id, stable_key, name, cwd, session_id, window_id, pane_id, last_seen_at)
                 VALUES('session', ?1, ?2, ?3, ?4, ?1, NULL, NULL, ?5)
                 ON CONFLICT(target_kind, tmux_id) DO UPDATE SET stable_key=excluded.stable_key, name=excluded.name, cwd=excluded.cwd, last_seen_at=excluded.last_seen_at",
                params![s.id, stable_session_key(s), s.name, s.path, ts],
            )?;
        }
        for w in windows {
            self.conn.execute(
                "INSERT INTO live_targets(target_kind, tmux_id, stable_key, name, cwd, session_id, window_id, pane_id, last_seen_at)
                 VALUES('window', ?1, ?2, ?3, ?4, ?5, ?1, NULL, ?6)
                 ON CONFLICT(target_kind, tmux_id) DO UPDATE SET stable_key=excluded.stable_key, name=excluded.name, cwd=excluded.cwd, session_id=excluded.session_id, last_seen_at=excluded.last_seen_at",
                params![w.id, stable_window_key(w), w.name, w.cwd, w.session_id, ts],
            )?;
        }
        for p in panes {
            self.conn.execute(
                "INSERT INTO live_targets(target_kind, tmux_id, stable_key, name, cwd, session_id, window_id, pane_id, last_seen_at)
                 VALUES('pane', ?1, ?2, ?3, ?4, ?5, ?6, ?1, ?7)
                 ON CONFLICT(target_kind, tmux_id) DO UPDATE SET stable_key=excluded.stable_key, name=excluded.name, cwd=excluded.cwd, session_id=excluded.session_id, window_id=excluded.window_id, last_seen_at=excluded.last_seen_at",
                params![p.id, stable_pane_key(p), p.title, p.cwd, p.session_id, p.window_id, ts],
            )?;
        }
        Ok(())
    }
}

pub fn stable_session_key(s: &SessionInfo) -> String {
    stable_session_parts(&s.path, &s.name)
}

pub fn stable_window_key(w: &WindowInfo) -> String {
    format!(
        "{}:window:{}",
        stable_session_parts(&w.session_path, &w.session_name),
        w.name
    )
}

pub fn stable_pane_key(p: &PaneInfo) -> String {
    format!(
        "{}:window:{}:pane:{}:{}:{}",
        stable_session_parts(&p.session_path, &p.session_name),
        p.window_name,
        p.title,
        p.cwd,
        p.command
    )
}

pub fn stable_session_parts(session_path: &str, session_name: &str) -> String {
    if !session_path.is_empty() {
        format!("cwd:{session_path}")
    } else {
        format!("session:{session_name}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_roundtrip() {
        let store = Store::open_in_memory().unwrap();
        store.set_note("session", "k", "hello").unwrap();
        assert_eq!(
            store.get_note("session", "k").unwrap().as_deref(),
            Some("hello")
        );
    }

    #[test]
    fn mru_dedupes_by_target() {
        let store = Store::open_in_memory().unwrap();
        store.push_mru("pane", "k1", Some("%1"), None).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        store.push_mru("pane", "k1", Some("%1"), None).unwrap();
        let recent = store.recent_mru(10).unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].tmux_target.as_deref(), Some("%1"));
    }
}
