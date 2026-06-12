use std::time::Duration;

use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::models::TaskType;

/// A task message placed on the queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMessage {
    pub task_id: String,
    pub task_text: String,
    pub task_type: Option<TaskType>,
    pub url: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub enqueued_at: i64,
}

/// Queue errors.
#[derive(Debug, thiserror::Error)]
pub enum QueueError {
    #[error("redis error: {0}")]
    Redis(#[from] redis::RedisError),

    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("r2d2 error: {0}")]
    R2d2(#[from] r2d2::Error),

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("no queue available")]
    NoQueueAvailable,
}

/// Queue trait — implemented by both Redis and SQLite backends.
#[async_trait::async_trait]
pub trait TaskQueue: Send + Sync + 'static {
    async fn put(&self, msg: &TaskMessage) -> Result<(), QueueError>;
    async fn pop(&self, timeout: Duration) -> Result<Option<TaskMessage>, QueueError>;
    async fn ack(&self, task_id: &str) -> Result<(), QueueError>;
    async fn depth(&self) -> Result<u64, QueueError>;
}

// ── Redis implementation ──────────────────────────────────────────────

pub struct RedisQueue {
    conn: tokio::sync::Mutex<redis::aio::MultiplexedConnection>,
}

impl RedisQueue {
    pub async fn new(url: &str) -> Result<Self, QueueError> {
        let client = redis::Client::open(url)?;
        let conn = client.get_multiplexed_async_connection().await?;
        Ok(Self {
            conn: tokio::sync::Mutex::new(conn),
        })
    }
}

#[async_trait::async_trait]
impl TaskQueue for RedisQueue {
    async fn put(&self, msg: &TaskMessage) -> Result<(), QueueError> {
        let payload = serde_json::to_string(msg)?;
        let mut conn = self.conn.lock().await;
        redis::AsyncCommands::rpush::<_, _, ()>(&mut *conn, "avid:tasks", &payload).await?;
        Ok(())
    }

    async fn pop(&self, timeout: Duration) -> Result<Option<TaskMessage>, QueueError> {
        let mut conn = self.conn.lock().await;
        let result: Option<(String, String)> =
            redis::AsyncCommands::blpop(&mut *conn, "avid:tasks", timeout.as_secs_f64()).await?;

        match result {
            Some((_key, payload)) => {
                let msg: TaskMessage = serde_json::from_str(&payload)?;
                redis::AsyncCommands::hset::<_, _, _, ()>(
                    &mut *conn,
                    "avid:inflight",
                    &msg.task_id,
                    &payload,
                )
                .await?;
                Ok(Some(msg))
            }
            None => Ok(None),
        }
    }

    async fn ack(&self, task_id: &str) -> Result<(), QueueError> {
        let mut conn = self.conn.lock().await;
        redis::AsyncCommands::hdel::<_, _, ()>(&mut *conn, "avid:inflight", task_id).await?;
        Ok(())
    }

    async fn depth(&self) -> Result<u64, QueueError> {
        let mut conn = self.conn.lock().await;
        let len: u64 = redis::AsyncCommands::llen(&mut *conn, "avid:tasks").await?;
        Ok(len)
    }
}

// ── SQLite fallback implementation ─────────────────────────────────────

pub struct SqliteQueue {
    pool: r2d2::Pool<r2d2_sqlite::SqliteConnectionManager>,
}

impl SqliteQueue {
    pub fn new(pool: r2d2::Pool<r2d2_sqlite::SqliteConnectionManager>) -> Self {
        Self { pool }
    }

    pub fn init_table(
        pool: &r2d2::Pool<r2d2_sqlite::SqliteConnectionManager>,
    ) -> Result<(), QueueError> {
        let conn = pool.get()?;
        let _ = conn.execute("PRAGMA busy_timeout = 5000", []);
        conn.execute(
            "CREATE TABLE IF NOT EXISTS queue (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                payload TEXT NOT NULL,
                state TEXT NOT NULL DEFAULT 'pending',
                enqueued_at INTEGER NOT NULL,
                claim_at INTEGER
            )",
            [],
        )?;
        // Migration: add task_id column if missing (idempotent)
        let _ = conn.execute("ALTER TABLE queue ADD COLUMN task_id TEXT", []);
        // Migration: add url column if missing
        let _ = conn.execute("ALTER TABLE queue ADD COLUMN url TEXT", []);
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_queue_state ON queue(state)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_queue_task_id ON queue(task_id)",
            [],
        )?;
        // Reset stale inflight rows on boot
        let cutoff = time::OffsetDateTime::now_utc().unix_timestamp() - 300;
        conn.execute(
            "UPDATE queue SET state = 'pending', claim_at = NULL WHERE state = 'inflight' AND claim_at < ?1",
            rusqlite::params![cutoff],
        )?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl TaskQueue for SqliteQueue {
    async fn put(&self, msg: &TaskMessage) -> Result<(), QueueError> {
        let payload = serde_json::to_string(msg)?;
        let task_id = msg.task_id.clone();
        let enqueued_at = msg.enqueued_at;
        let pool = self.pool.clone();

        let mut last_err = None;
        for i in 0..5 {
            if i > 0 {
                tokio::time::sleep(Duration::from_millis(100 * 2u64.pow(i - 1))).await;
            }
            let payload = payload.clone();
            let task_id = task_id.clone();
            let pool_inner = pool.clone();
            let res = tokio::task::spawn_blocking(move || {
                let conn = pool_inner.get().map_err(QueueError::R2d2)?;
                let _ = conn.execute("PRAGMA busy_timeout = 5000", []);
                conn.execute(
                    "INSERT INTO queue (task_id, payload, state, enqueued_at) VALUES (?1, ?2, 'pending', ?3)",
                    rusqlite::params![task_id, payload, enqueued_at],
                )?;
                Ok(())
            }).await;

            match res {
                Ok(Ok(())) => return Ok(()),
                Ok(Err(e)) => {
                    if is_locked_err(&e) {
                        last_err = Some(e);
                        continue;
                    }
                    return Err(e);
                }
                Err(e) => {
                    return Err(QueueError::Sqlite(rusqlite::Error::ToSqlConversionFailure(
                        Box::new(e),
                    )))
                }
            }
        }
        Err(last_err.unwrap())
    }

    async fn pop(&self, timeout: Duration) -> Result<Option<TaskMessage>, QueueError> {
        let pool = self.pool.clone();
        let deadline =
            time::OffsetDateTime::now_utc() + time::Duration::seconds_f64(timeout.as_secs_f64());

        loop {
            let mut last_err = None;
            let mut success = false;
            let mut msg_out = None;

            for i in 0..5 {
                if i > 0 {
                    tokio::time::sleep(Duration::from_millis(50 * 2u64.pow(i - 1))).await;
                }
                let pool_inner = pool.clone();
                let res = tokio::task::spawn_blocking(move || {
                    let conn = pool_inner.get().map_err(QueueError::R2d2)?;
                    let _ = conn.execute("PRAGMA busy_timeout = 5000", []);
                    let now = time::OffsetDateTime::now_utc().unix_timestamp();
                    let mut stmt = conn.prepare(
                        "SELECT id, task_id, payload FROM queue WHERE state = 'pending' ORDER BY enqueued_at ASC LIMIT 1",
                    )?;
                    let mut rows = stmt.query([])?;
                    if let Some(row) = rows.next()? {
                        let id: i64 = row.get(0)?;
                        let payload: String = row.get(2)?;
                        conn.execute(
                            "UPDATE queue SET state = 'inflight', claim_at = ?1 WHERE id = ?2",
                            rusqlite::params![now, id],
                        )?;
                        let msg: TaskMessage = serde_json::from_str(&payload)?;
                        Ok(Some(msg))
                    } else {
                        Ok(None)
                    }
                }).await;

                match res {
                    Ok(Ok(m)) => {
                        msg_out = m;
                        success = true;
                        break;
                    }
                    Ok(Err(e)) => {
                        if is_locked_err(&e) {
                            last_err = Some(e);
                            continue;
                        }
                        return Err(e);
                    }
                    Err(e) => {
                        return Err(QueueError::Sqlite(rusqlite::Error::ToSqlConversionFailure(
                            Box::new(e),
                        )))
                    }
                }
            }

            if success {
                if msg_out.is_some() {
                    return Ok(msg_out);
                }
                if time::OffsetDateTime::now_utc() >= deadline {
                    return Ok(None);
                }
                tokio::time::sleep(Duration::from_millis(200)).await;
                continue;
            }
            return Err(last_err.unwrap());
        }
    }

    async fn ack(&self, task_id: &str) -> Result<(), QueueError> {
        let task_id = task_id.to_string();
        let pool = self.pool.clone();

        let mut last_err = None;
        for i in 0..5 {
            if i > 0 {
                tokio::time::sleep(Duration::from_millis(100 * 2u64.pow(i - 1))).await;
            }
            let task_id = task_id.clone();
            let pool_inner = pool.clone();
            let res = tokio::task::spawn_blocking(move || {
                let conn = pool_inner.get().map_err(QueueError::R2d2)?;
                let _ = conn.execute("PRAGMA busy_timeout = 5000", []);
                conn.execute(
                    "UPDATE queue SET state = 'done' WHERE state = 'inflight' AND task_id = ?1",
                    rusqlite::params![task_id],
                )?;
                Ok(())
            })
            .await;

            match res {
                Ok(Ok(())) => return Ok(()),
                Ok(Err(e)) => {
                    if is_locked_err(&e) {
                        last_err = Some(e);
                        continue;
                    }
                    return Err(e);
                }
                Err(e) => {
                    return Err(QueueError::Sqlite(rusqlite::Error::ToSqlConversionFailure(
                        Box::new(e),
                    )))
                }
            }
        }
        Err(last_err.unwrap())
    }

    async fn depth(&self) -> Result<u64, QueueError> {
        let pool = self.pool.clone();

        let mut last_err = None;
        for i in 0..5 {
            if i > 0 {
                tokio::time::sleep(Duration::from_millis(100 * 2u64.pow(i - 1))).await;
            }
            let pool_inner = pool.clone();
            let res = tokio::task::spawn_blocking(move || {
                let conn = pool_inner.get().map_err(QueueError::R2d2)?;
                let _ = conn.execute("PRAGMA busy_timeout = 5000", []);
                let count: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM queue WHERE state = 'pending'",
                    [],
                    |row| row.get(0),
                )?;
                Ok(count as u64)
            })
            .await;

            match res {
                Ok(Ok(d)) => return Ok(d),
                Ok(Err(e)) => {
                    if is_locked_err(&e) {
                        last_err = Some(e);
                        continue;
                    }
                    return Err(e);
                }
                Err(e) => {
                    return Err(QueueError::Sqlite(rusqlite::Error::ToSqlConversionFailure(
                        Box::new(e),
                    )))
                }
            }
        }
        Err(last_err.unwrap())
    }
}

fn is_locked_err(e: &QueueError) -> bool {
    match e {
        QueueError::Sqlite(rusqlite::Error::SqliteFailure(err, _)) => {
            err.code == rusqlite::ErrorCode::DatabaseLocked
                || err.code == rusqlite::ErrorCode::DatabaseBusy
        }
        _ => false,
    }
}

/// Try Redis first, then fall back to SQLite.
pub async fn create_queue(
    redis_url: &Option<String>,
    sqlite_pool: r2d2::Pool<r2d2_sqlite::SqliteConnectionManager>,
) -> Result<Box<dyn TaskQueue>, QueueError> {
    if let Some(url) = redis_url {
        match RedisQueue::new(url).await {
            Ok(q) => {
                debug!("using Redis queue backend");
                return Ok(Box::new(q));
            }
            Err(e) => {
                debug!("Redis unavailable ({}), falling back to SQLite", e);
            }
        }
    }
    debug!("using SQLite queue backend");
    Ok(Box::new(SqliteQueue::new(sqlite_pool)))
}
