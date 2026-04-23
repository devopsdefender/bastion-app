//! Per-device SQLite-backed session log + FTS5 search.
//!
//! Every byte a remote attach session delivers passes through
//! `append_bytes`: ANSI is stripped, the stripped text goes into a
//! FTS5 virtual table, and the session's `last_seen_ms` is bumped.
//! Raw bytes are **not** retained in v0 (keeps DB small; no replay
//! feature yet).
//!
//! Search is `SELECT ... snippet(...) FROM session_fts WHERE ... MATCH`.
//! Session metadata (agent, tmux_name, kind, title, first/last seen)
//! lives in a separate `sessions` table so a row survives after the
//! live tmux session is gone.
//!
//! Writes and queries both go through `spawn_blocking` around a
//! std-mutex'd `rusqlite::Connection`. v0 is cheap enough; if the
//! byte-pump ever contends with search, switch to a batched writer
//! task fed by an mpsc.

use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use serde::Serialize;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS sessions (
    id            TEXT PRIMARY KEY,
    agent_origin  TEXT NOT NULL,
    tmux_name     TEXT NOT NULL,
    kind          TEXT,
    title         TEXT,
    first_seen_ms INTEGER NOT NULL,
    last_seen_ms  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS ix_sessions_agent_name ON sessions(agent_origin, tmux_name);

CREATE VIRTUAL TABLE IF NOT EXISTS session_fts USING fts5(
    session_id UNINDEXED,
    ts_ms UNINDEXED,
    text,
    tokenize='unicode61'
);
"#;

#[derive(Clone)]
pub struct SessionLog {
    db: Arc<StdMutex<Connection>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    pub session_id: String,
    pub agent_origin: String,
    pub tmux_name: String,
    pub ts_ms: i64,
    pub snippet: String,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

impl SessionLog {
    pub fn open(config_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(config_dir).ok();
        let path = config_dir.join("bastion.db");
        let conn = Connection::open(&path).with_context(|| format!("open {}", path.display()))?;
        conn.execute_batch(SCHEMA).context("init schema")?;
        conn.pragma_update(None, "journal_mode", "WAL").ok();
        conn.pragma_update(None, "synchronous", "NORMAL").ok();
        Ok(Self {
            db: Arc::new(StdMutex::new(conn)),
        })
    }

    pub async fn register_session(
        &self,
        id: String,
        agent_origin: String,
        tmux_name: String,
        kind: Option<String>,
    ) -> Result<()> {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let c = db.lock().map_err(|e| anyhow::anyhow!("poisoned: {e}"))?;
            let now = now_ms();
            c.execute(
                "INSERT INTO sessions (id, agent_origin, tmux_name, kind, title, first_seen_ms, last_seen_ms)
                 VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?5)
                 ON CONFLICT(id) DO UPDATE SET
                    agent_origin = excluded.agent_origin,
                    tmux_name    = excluded.tmux_name,
                    kind         = COALESCE(excluded.kind, sessions.kind),
                    last_seen_ms = excluded.last_seen_ms",
                params![id, agent_origin, tmux_name, kind, now],
            )?;
            Ok(())
        })
        .await
        .context("join")?
    }

    pub async fn append_bytes(&self, session_id: String, bytes: Vec<u8>) -> Result<()> {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let stripped = strip_ansi_escapes::strip(&bytes);
            let text = String::from_utf8_lossy(&stripped).into_owned();
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return Ok(());
            }
            let c = db.lock().map_err(|e| anyhow::anyhow!("poisoned: {e}"))?;
            let now = now_ms();
            c.execute(
                "INSERT INTO session_fts (session_id, ts_ms, text) VALUES (?1, ?2, ?3)",
                params![session_id, now, trimmed],
            )?;
            c.execute(
                "UPDATE sessions SET last_seen_ms = ?1 WHERE id = ?2",
                params![now, session_id],
            )?;
            Ok(())
        })
        .await
        .context("join")?
    }

    #[allow(dead_code)] // wired for future "rename session" Tauri command
    pub async fn set_title(&self, session_id: String, title: String) -> Result<()> {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let c = db.lock().map_err(|e| anyhow::anyhow!("poisoned: {e}"))?;
            c.execute(
                "UPDATE sessions SET title = ?1 WHERE id = ?2",
                params![title, session_id],
            )?;
            Ok(())
        })
        .await
        .context("join")?
    }

    pub async fn search(&self, query: String, limit: usize) -> Result<Vec<SearchHit>> {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || -> Result<Vec<SearchHit>> {
            let c = db.lock().map_err(|e| anyhow::anyhow!("poisoned: {e}"))?;
            let mut stmt = c.prepare(
                "SELECT f.session_id,
                        COALESCE(s.agent_origin, ''),
                        COALESCE(s.tmux_name, ''),
                        f.ts_ms,
                        snippet(session_fts, 2, '[', ']', '…', 10)
                 FROM session_fts f
                 LEFT JOIN sessions s ON s.id = f.session_id
                 WHERE session_fts MATCH ?1
                 ORDER BY f.ts_ms DESC
                 LIMIT ?2",
            )?;
            let rows = stmt
                .query_map(params![query, limit as i64], |row| {
                    Ok(SearchHit {
                        session_id: row.get(0)?,
                        agent_origin: row.get(1)?,
                        tmux_name: row.get(2)?,
                        ts_ms: row.get(3)?,
                        snippet: row.get(4)?,
                    })
                })?
                .filter_map(|r| r.ok())
                .collect::<Vec<_>>();
            Ok(rows)
        })
        .await
        .context("join")?
    }
}
