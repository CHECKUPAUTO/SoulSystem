//! Persistence — Stockage durable NVMe/SQLite pour l'état

use rusqlite::Connection;
use serde_json;
use std::path::Path;
use tracing::info;

const DB_PATH: &str = "/mnt/nvme/soullink_brain/openclaw_u_state.db";

pub struct Persistence {
    conn: Connection,
}

impl Persistence {
    pub fn new() -> Result<Self, String> {
        let db_dir = Path::new(DB_PATH).parent().unwrap();
        std::fs::create_dir_all(db_dir).map_err(|e| format!("mkdir: {}", e))?;

        let conn = Connection::open(DB_PATH).map_err(|e| format!("sqlite open: {}", e))?;

        let p = Self { conn };
        p.init_tables()?;
        info!("💾 Persistence SQLite initialisée: {}", DB_PATH);
        Ok(p)
    }

    fn init_tables(&self) -> Result<(), String> {
        self.conn
            .execute_batch(
                r#"
            CREATE TABLE IF NOT EXISTS state (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                version TEXT NOT NULL,
                energy REAL NOT NULL,
                cycles INTEGER NOT NULL,
                tasks INTEGER NOT NULL,
                evolutions INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                json BLOB NOT NULL
            );
            CREATE TABLE IF NOT EXISTS history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT NOT NULL,
                event TEXT NOT NULL,
                energy_delta REAL NOT NULL,
                outcome TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS q_table (
                action TEXT PRIMARY KEY,
                total_reward REAL NOT NULL,
                count INTEGER NOT NULL,
                success_rate REAL NOT NULL,
                avg_reward REAL NOT NULL
            );
            CREATE TABLE IF NOT EXISTS resilience (
                action TEXT PRIMARY KEY,
                failures INTEGER NOT NULL,
                last_error TEXT,
                last_seen TEXT
            );
        "#,
            )
            .map_err(|e| format!("init tables: {}", e))?;
        Ok(())
    }

    pub fn save_state(&self, state: &crate::CoreState) -> Result<(), String> {
        let json = serde_json::to_string(state).map_err(|e| format!("serialize: {}", e))?;

        self.conn.execute(
            r#"INSERT INTO state (id, version, energy, cycles, tasks, evolutions, created_at, updated_at, json)
               VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
               ON CONFLICT(id) DO UPDATE SET
               version = excluded.version,
               energy = excluded.energy,
               cycles = excluded.cycles,
               tasks = excluded.tasks,
               evolutions = excluded.evolutions,
               updated_at = excluded.updated_at,
               json = excluded.json"#,
            rusqlite::params![
                state.version,
                state.energy,
                state.uptime_cycles as i64,
                state.task_count as i64,
                state.evolution_count as i64,
                state.birth_time.to_rfc3339(),
                chrono::Utc::now().to_rfc3339(),
                json
            ],
        ).map_err(|e| format!("save state: {}", e))?;

        Ok(())
    }

    pub fn load_state(&self) -> Result<Option<crate::CoreState>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT json FROM state WHERE id = 1")
            .map_err(|e| format!("prepare: {}", e))?;

        let mut rows = stmt.query([]).map_err(|e| format!("query: {}", e))?;

        if let Some(row) = rows.next().map_err(|e| format!("next: {}", e))? {
            let json: String = row.get(0).map_err(|e| format!("get: {}", e))?;
            let state: crate::CoreState =
                serde_json::from_str(&json).map_err(|e| format!("deserialize: {}", e))?;
            info!(
                "💾 État chargé depuis SQLite — cycles:{}",
                state.uptime_cycles
            );
            return Ok(Some(state));
        }

        Ok(None)
    }

    pub fn append_history(&self, entry: &crate::HistoryEntry) -> Result<(), String> {
        self.conn.execute(
            "INSERT INTO history (timestamp, event, energy_delta, outcome) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                entry.timestamp.to_rfc3339(),
                entry.event,
                entry.energy_delta,
                entry.outcome
            ],
        ).map_err(|e| format!("append history: {}", e))?;
        Ok(())
    }

    pub fn save_q_table(&self, q_table: &crate::learning::QTable) -> Result<(), String> {
        let tx = self
            .conn
            .unchecked_transaction()
            .map_err(|e| format!("transaction: {}", e))?;

        for (action, score) in &q_table.scores {
            tx.execute(
                r#"INSERT INTO q_table (action, total_reward, count, success_rate, avg_reward)
                   VALUES (?1, ?2, ?3, ?4, ?5)
                   ON CONFLICT(action) DO UPDATE SET
                   total_reward = excluded.total_reward,
                   count = excluded.count,
                   success_rate = excluded.success_rate,
                   avg_reward = excluded.avg_reward"#,
                rusqlite::params![
                    action,
                    score.total_reward,
                    score.count as i64,
                    score.success_rate,
                    score.avg_reward
                ],
            )
            .map_err(|e| format!("save q: {}", e))?;
        }

        tx.commit().map_err(|e| format!("commit: {}", e))?;
        Ok(())
    }

    pub fn export_to_json(&self, path: &str) -> Result<(), String> {
        if let Ok(Some(state)) = self.load_state() {
            let json =
                serde_json::to_string_pretty(&state).map_err(|e| format!("serialize: {}", e))?;
            std::fs::write(path, json).map_err(|e| format!("write: {}", e))?;
            info!("💾 Export JSON: {}", path);
        }
        Ok(())
    }

    pub fn vacuum(&self) -> Result<(), String> {
        self.conn
            .execute("VACUUM", [])
            .map_err(|e| format!("vacuum: {}", e))?;
        info!("💾 SQLite VACUUM terminé");
        Ok(())
    }
}
