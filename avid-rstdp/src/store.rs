use crate::pattern::PatternSignature;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;
use std::time::SystemTime;

#[derive(Debug, thiserror::Error)]
pub enum PatternStoreError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("pool: {0}")]
    Pool(#[from] r2d2::Error),
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PatternStats {
    pub hash: String,
    pub canonical: String,
    pub seen_count: u64,
    pub success_count: u64,
    pub originality_sum: f64,
    pub last_seen: i64,
    pub ncpu_compiled: bool,
}

impl PatternStats {
    #[must_use]
    pub fn success_rate(&self) -> f32 {
        if self.seen_count == 0 {
            0.0
        } else {
            self.success_count as f32 / self.seen_count as f32
        }
    }

    #[must_use]
    pub fn originality_avg(&self) -> f32 {
        if self.seen_count == 0 {
            0.0
        } else {
            (self.originality_sum / self.seen_count as f64) as f32
        }
    }
}

pub struct PatternStore {
    pool: Pool<SqliteConnectionManager>,
}

impl PatternStore {
    pub fn new(db_path: &str) -> Result<Self, PatternStoreError> {
        let manager = SqliteConnectionManager::file(db_path);
        let pool = Pool::builder().max_size(4).build(manager)?;
        Self::init(&pool)?;
        Ok(Self { pool })
    }

    fn init(pool: &Pool<SqliteConnectionManager>) -> Result<(), PatternStoreError> {
        let conn = pool.get()?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             CREATE TABLE IF NOT EXISTS patterns (
                 hash TEXT PRIMARY KEY,
                 canonical TEXT NOT NULL,
                 seen_count INTEGER NOT NULL DEFAULT 0,
                 success_count INTEGER NOT NULL DEFAULT 0,
                 originality_sum REAL NOT NULL DEFAULT 0,
                 last_seen INTEGER NOT NULL,
                 ncpu_compiled INTEGER NOT NULL DEFAULT 0
             );
             CREATE INDEX IF NOT EXISTS idx_patterns_compile ON patterns(ncpu_compiled, success_count);",
        )?;
        Ok(())
    }

    pub fn record(
        &self,
        sig: &PatternSignature,
        sandbox_success: bool,
        originality: f32,
    ) -> Result<PatternStats, PatternStoreError> {
        let conn = self.pool.get()?;
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        conn.execute(
            "INSERT INTO patterns (hash, canonical, seen_count, success_count, originality_sum, last_seen)
             VALUES (?1, ?2, 1, ?3, ?4, ?5)
             ON CONFLICT(hash) DO UPDATE SET
                 seen_count = seen_count + 1,
                 success_count = success_count + ?3,
                 originality_sum = originality_sum + ?4,
                 last_seen = ?5",
            params![sig.hash, sig.canonical, i64::from(sandbox_success), f64::from(originality), now],
        )?;

        let mut stmt = conn.prepare(
            "SELECT hash, canonical, seen_count, success_count, originality_sum, last_seen, ncpu_compiled
             FROM patterns WHERE hash = ?1",
        )?;
        let stats = stmt.query_row(params![sig.hash], |row| {
            Ok(PatternStats {
                hash: row.get(0)?,
                canonical: row.get(1)?,
                seen_count: row.get::<_, i64>(2)? as u64,
                success_count: row.get::<_, i64>(3)? as u64,
                originality_sum: row.get(4)?,
                last_seen: row.get(5)?,
                ncpu_compiled: row.get::<_, i64>(6)? != 0,
            })
        })?;
        Ok(stats)
    }

    pub fn mark_compiled(&self, hash: &str) -> Result<(), PatternStoreError> {
        let conn = self.pool.get()?;
        conn.execute(
            "UPDATE patterns SET ncpu_compiled = 1 WHERE hash = ?1",
            params![hash],
        )?;
        Ok(())
    }
}
