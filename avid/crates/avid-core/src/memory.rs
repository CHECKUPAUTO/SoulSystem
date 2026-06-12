use std::collections::BTreeMap;
use std::path::Path;

use rusqlite::params;
use serde::{Deserialize, Serialize};
use tracing::{debug, error};

/// Memory errors.
#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("r2d2 error: {0}")]
    R2d2(#[from] r2d2::Error),

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("pool error: {0}")]
    Pool(String),
}

/// Memory store backed by SQLite.
#[derive(Clone)]
pub struct MemoryStore {
    pool: r2d2::Pool<r2d2_sqlite::SqliteConnectionManager>,
}

/// A recalled past task for context injection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecallEntry {
    pub task_text: String,
    pub status: String,
    pub payload_summary: String,
}

impl MemoryStore {
    /// Create a new memory store, ensuring the database schema exists.
    pub fn new(db_path: &Path, pool_size: u32) -> Result<Self, MemoryError> {
        let manager = r2d2_sqlite::SqliteConnectionManager::file(db_path).with_flags(
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE
                | rusqlite::OpenFlags::SQLITE_OPEN_CREATE
                | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        );

        let pool = r2d2::Pool::builder().max_size(pool_size).build(manager)?;

        let conn = pool.get()?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;",
        )?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS patterns(
                id INTEGER PRIMARY KEY,
                task_hash TEXT NOT NULL,
                fingerprint_json TEXT NOT NULL,
                accepted_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS results(
                id INTEGER PRIMARY KEY,
                task_id TEXT UNIQUE NOT NULL,
                status TEXT NOT NULL CHECK(status IN
                    ('accepted','rejected','execution_error','timeout','split')),
                payload_json TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS traces(
                id INTEGER PRIMARY KEY,
                task_id TEXT NOT NULL,
                agent TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                ts INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_traces_task ON traces(task_id);",
        )?;

        debug!("memory store initialized at {}", db_path.display());
        Ok(Self { pool })
    }

    /// Store a fingerprint alongside its task hash.
    pub async fn store_fingerprint(
        &self,
        task_hash: &str,
        fingerprint: &avid_anticlone::Fingerprint,
    ) -> Result<i64, MemoryError> {
        let fp_json = serde_json::to_string(fingerprint)?;
        let pool = self.pool.clone();
        let task_hash = task_hash.to_string();
        let now = time::OffsetDateTime::now_utc().unix_timestamp();

        let id = tokio::task::spawn_blocking(move || -> Result<i64, MemoryError> {
            let conn = pool.get().map_err(|e| MemoryError::Pool(e.to_string()))?;
            conn.execute(
                "INSERT INTO patterns (task_hash, fingerprint_json, accepted_at) VALUES (?1, ?2, ?3)",
                params![task_hash, fp_json, now],
            )?;
            Ok(conn.last_insert_rowid())
        })
        .await
        .map_err(|e| MemoryError::Pool(format!("join error: {e}")))?;
        id
    }

    /// Store a result row.
    pub async fn store_result(
        &self,
        task_id: &str,
        status: &str,
        payload: &serde_json::Value,
    ) -> Result<(), MemoryError> {
        let payload_json = serde_json::to_string(payload)?;
        let pool = self.pool.clone();
        let task_id = task_id.to_string();
        let status = status.to_string();
        let now = time::OffsetDateTime::now_utc().unix_timestamp();

        tokio::task::spawn_blocking(move || -> Result<(), MemoryError> {
            let conn = pool.get().map_err(|e| MemoryError::Pool(e.to_string()))?;
            conn.execute(
                "INSERT OR REPLACE INTO results (task_id, status, payload_json, created_at) VALUES (?1, ?2, ?3, ?4)",
                params![task_id, status, payload_json, now],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| MemoryError::Pool(format!("join error: {e}")))??;
        Ok(())
    }

    /// Store a trace entry.
    pub async fn store_trace(
        &self,
        task_id: &str,
        agent: &str,
        payload: &serde_json::Value,
    ) -> Result<(), MemoryError> {
        let payload_json = serde_json::to_string(payload)?;
        let pool = self.pool.clone();
        let task_id = task_id.to_string();
        let agent = agent.to_string();
        let now = time::OffsetDateTime::now_utc().unix_timestamp();

        tokio::task::spawn_blocking(move || -> Result<(), MemoryError> {
            let conn = pool.get().map_err(|e| MemoryError::Pool(e.to_string()))?;
            conn.execute(
                "INSERT INTO traces (task_id, agent, payload_json, ts) VALUES (?1, ?2, ?3, ?4)",
                params![task_id, agent, payload_json, now],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| MemoryError::Pool(format!("join error: {e}")))??;
        Ok(())
    }

    /// Recall top-K similar past tasks using TF-IDF cosine similarity.
    pub async fn recall(&self, task_text: &str, k: usize) -> Result<Vec<RecallEntry>, MemoryError> {
        let pool = self.pool.clone();
        let task_text = task_text.to_string();

        tokio::task::spawn_blocking(move || -> Result<Vec<RecallEntry>, MemoryError> {
            let conn = pool.get().map_err(|e| MemoryError::Pool(e.to_string()))?;

            let mut stmt = conn
                .prepare("SELECT payload_json FROM results ORDER BY created_at DESC LIMIT 500")?;
            let mut candidates = Vec::new();
            let rows = stmt.query_map([], |row| {
                let json_str: String = row.get(0)?;
                Ok(json_str)
            })?;

            for row in rows {
                let json_str = row?;
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&json_str) {
                    let past_task = val
                        .get("task")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let status = val
                        .get("status")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string();
                    let summary = val
                        .get("summary")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    if !past_task.is_empty() {
                        let sim = tfidf_cosine(&task_text, &past_task);
                        candidates.push((
                            sim,
                            RecallEntry {
                                task_text: past_task,
                                status,
                                payload_summary: summary,
                            },
                        ));
                    }
                }
            }

            candidates.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
            candidates.truncate(k);
            Ok(candidates.into_iter().map(|(_, entry)| entry).collect())
        })
        .await
        .map_err(|e| MemoryError::Pool(format!("join error: {e}")))?
    }

    /// List recent results.
    pub async fn list_results(&self, limit: usize) -> Result<Vec<serde_json::Value>, MemoryError> {
        let pool = self.pool.clone();

        tokio::task::spawn_blocking(move || -> Result<Vec<serde_json::Value>, MemoryError> {
            let conn = pool.get().map_err(|e| MemoryError::Pool(e.to_string()))?;
            let mut stmt =
                conn.prepare("SELECT payload_json FROM results ORDER BY created_at DESC LIMIT ?1")?;
            let rows = stmt.query_map(params![limit], |row| {
                let json_str: String = row.get(0)?;
                Ok(json_str)
            })?;

            let mut results = Vec::new();
            for row in rows {
                let json_str = row?;
                results.push(serde_json::from_str(&json_str)?);
            }
            Ok(results)
        })
        .await
        .map_err(|e| MemoryError::Pool(format!("join error: {e}")))?
    }

    /// Get all fingerprints from the database for anti-clone checking.
    pub async fn get_all_fingerprints(
        &self,
    ) -> Result<Vec<(i64, avid_anticlone::Fingerprint)>, MemoryError> {
        let pool = self.pool.clone();

        tokio::task::spawn_blocking(
            move || -> Result<Vec<(i64, avid_anticlone::Fingerprint)>, MemoryError> {
                let conn = pool.get().map_err(|e| MemoryError::Pool(e.to_string()))?;
                let mut stmt =
                    conn.prepare("SELECT id, fingerprint_json FROM patterns ORDER BY id")?;
                let rows = stmt.query_map([], |row| {
                    let id: i64 = row.get(0)?;
                    let json_str: String = row.get(1)?;
                    Ok((id, json_str))
                })?;

                let mut fps = Vec::new();
                for row in rows {
                    let (id, json_str) = row?;
                    match serde_json::from_str::<avid_anticlone::Fingerprint>(&json_str) {
                        Ok(fp) => fps.push((id, fp)),
                        Err(e) => {
                            error!("failed to deserialize fingerprint {id}: {e}");
                        }
                    }
                }
                Ok(fps)
            },
        )
        .await
        .map_err(|e| MemoryError::Pool(format!("join error: {e}")))?
    }

    /// Get a result by task_id.
    pub async fn get_result(
        &self,
        task_id: &str,
    ) -> Result<Option<serde_json::Value>, MemoryError> {
        let pool = self.pool.clone();
        let task_id = task_id.to_string();

        tokio::task::spawn_blocking(move || -> Result<Option<serde_json::Value>, MemoryError> {
            let conn = pool.get().map_err(|e| MemoryError::Pool(e.to_string()))?;
            let mut stmt = conn.prepare("SELECT payload_json FROM results WHERE task_id = ?1")?;
            let mut rows = stmt.query(params![task_id])?;
            if let Some(row) = rows.next()? {
                let json_str: String = row.get(0)?;
                Ok(Some(serde_json::from_str(&json_str)?))
            } else {
                Ok(None)
            }
        })
        .await
        .map_err(|e| MemoryError::Pool(format!("join error: {e}")))?
    }

    /// Check if memory is healthy.
    pub async fn health_check(&self) -> bool {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || pool.get().is_ok())
            .await
            .unwrap_or(false)
    }
}

/// Simple TF-IDF cosine similarity using term frequency vectors.
fn tfidf_cosine(a: &str, b: &str) -> f32 {
    let tf_a = term_frequencies(a);
    let tf_b = term_frequencies(b);

    let all_terms: std::collections::BTreeSet<&String> = tf_a.keys().chain(tf_b.keys()).collect();

    let mut dot = 0.0_f32;
    let mut norm_a = 0.0_f32;
    let mut norm_b = 0.0_f32;

    for term in all_terms {
        let va = tf_a.get(term).copied().unwrap_or(0.0);
        let vb = tf_b.get(term).copied().unwrap_or(0.0);
        dot += va * vb;
        norm_a += va * va;
        norm_b += vb * vb;
    }

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a.sqrt() * norm_b.sqrt())
}

fn term_frequencies(text: &str) -> BTreeMap<String, f32> {
    let mut tf = BTreeMap::<String, u32>::new();
    let mut total = 0_u32;
    for word in text.split_whitespace() {
        let cleaned = word
            .to_lowercase()
            .trim_matches(|c: char| !c.is_alphanumeric())
            .to_string();
        if cleaned.len() >= 2 {
            *tf.entry(cleaned).or_insert(0) += 1;
            total += 1;
        }
    }
    let total = total.max(1) as f32;
    tf.into_iter().map(|(k, v)| (k, v as f32 / total)).collect()
}
