//! Persistence SQLite des statuts de bridges
//!
//! Schéma :
//!   CREATE TABLE bridge_status (
//!     name TEXT PRIMARY KEY,
//!     reachable INTEGER NOT NULL,
//!     version TEXT,
//!     last_check INTEGER NOT NULL,
//!     consecutive_failures INTEGER NOT NULL DEFAULT 0,
//!     last_error TEXT
//!   );
//!
//! Usage :
//!   let store = BridgeStore::open("/var/lib/soulsystem/bridges.db")?;
//!   store.record("avid", true, Some("0.6.0"))?;
//!   let history = store.history("avid", 10)?; // 10 derniers

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct BridgeStatusRecord {
    pub name: String,
    pub reachable: bool,
    pub version: Option<String>,
    pub last_check: i64,
    pub consecutive_failures: u32,
    pub last_error: Option<String>,
}

pub struct BridgeStore {
    conn: Arc<Mutex<Connection>>,
}

impl BridgeStore {
    /// Ouvre ou crée la base SQLite au chemin donné.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent).context("create db parent dir")?;
        }
        let conn = Connection::open(&path).context("open sqlite")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS bridge_status (
                name TEXT PRIMARY KEY,
                reachable INTEGER NOT NULL,
                version TEXT,
                last_check INTEGER NOT NULL,
                consecutive_failures INTEGER NOT NULL DEFAULT 0,
                last_error TEXT
            );
            CREATE TABLE IF NOT EXISTS bridge_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                reachable INTEGER NOT NULL,
                version TEXT,
                ts INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_history_name_ts ON bridge_history(name, ts DESC);",
        )
        .context("create schema")?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Enregistre le statut courant d'un bridge.
    pub fn record(
        &self,
        name: &str,
        reachable: bool,
        version: Option<&str>,
        error: Option<&str>,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().timestamp();
        // Update or insert
        let prev_failures: i32 = conn
            .query_row(
                "SELECT consecutive_failures FROM bridge_status WHERE name = ?1",
                params![name],
                |r| r.get(0),
            )
            .optional()
            .context("read prev failures")?
            .unwrap_or(0);

        let new_failures = if reachable {
            0
        } else {
            (prev_failures + 1).min(1000)
        };

        conn.execute(
            "INSERT INTO bridge_status(name, reachable, version, last_check, consecutive_failures, last_error)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(name) DO UPDATE SET
                reachable = excluded.reachable,
                version = excluded.version,
                last_check = excluded.last_check,
                consecutive_failures = excluded.consecutive_failures,
                last_error = excluded.last_error",
            params![name, reachable as i32, version, now, new_failures, error],
        )
        .context("upsert bridge_status")?;

        conn.execute(
            "INSERT INTO bridge_history(name, reachable, version, ts) VALUES (?1, ?2, ?3, ?4)",
            params![name, reachable as i32, version, now],
        )
        .context("insert bridge_history")?;

        Ok(())
    }

    /// Renvoie l'historique récent d'un bridge.
    pub fn history(&self, name: &str, limit: u32) -> Result<Vec<BridgeStatusRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT name, reachable, version, ts
                 FROM bridge_history WHERE name = ?1
                 ORDER BY ts DESC LIMIT ?2",
            )
            .context("prepare history")?;
        let rows = stmt
            .query_map(params![name, limit], |r| {
                Ok(BridgeStatusRecord {
                    name: r.get(0)?,
                    reachable: r.get::<_, i32>(1)? != 0,
                    version: r.get(2)?,
                    last_check: r.get(3)?,
                    consecutive_failures: 0,
                    last_error: None,
                })
            })
            .context("query history")?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Renvoie le statut actuel de tous les bridges.
    pub fn all(&self) -> Result<Vec<BridgeStatusRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT name, reachable, version, last_check, consecutive_failures, last_error
                 FROM bridge_status ORDER BY name",
            )
            .context("prepare all")?;
        let rows = stmt
            .query_map(params![], |r| {
                Ok(BridgeStatusRecord {
                    name: r.get(0)?,
                    reachable: r.get::<_, i32>(1)? != 0,
                    version: r.get(2)?,
                    last_check: r.get(3)?,
                    consecutive_failures: r.get::<_, i32>(4)? as u32,
                    last_error: r.get(5)?,
                })
            })
            .context("query all")?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_open_record_read() {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().with_extension("db");
        let store = BridgeStore::open(&path).unwrap();
        store.record("avid", true, Some("0.6.0"), None).unwrap();
        store.record("openevolve", false, None, Some("timeout")).unwrap();

        let all = store.all().unwrap();
        assert_eq!(all.len(), 2);
        assert!(all.iter().any(|r| r.name == "avid" && r.reachable));
        assert!(all.iter().any(|r| r.name == "openevolve" && !r.reachable && r.consecutive_failures == 1));
    }

    #[test]
    fn test_history() {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().with_extension("db");
        let store = BridgeStore::open(&path).unwrap();
        for i in 0..5 {
            store.record("test", i % 2 == 0, None, None).unwrap();
        }
        let h = store.history("test", 3).unwrap();
        assert_eq!(h.len(), 3);
    }
}
