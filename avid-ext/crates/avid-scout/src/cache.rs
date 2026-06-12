use serde::{Deserialize, Serialize};

/// Persistent page cache backed by SQLite.
#[derive(Debug)]
pub struct PageCache {
    pool: r2d2::Pool<r2d2_sqlite::SqliteConnectionManager>,
}

/// Cached page record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    pub url: String,
    pub title: Option<String>,
    pub text: String,
    pub fetched_at: i64,
}

impl PageCache {
    /// Open or create a disk-backed page cache.
    pub fn new(path: &str) -> Result<Self, rusqlite::Error> {
        let manager = r2d2_sqlite::SqliteConnectionManager::file(path);
        let pool = r2d2::Pool::builder()
            .build(manager)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        let conn = pool
            .get()
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS page_cache (
                url TEXT PRIMARY KEY,
                title TEXT,
                text TEXT NOT NULL,
                fetched_at INTEGER NOT NULL
            )",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_page_cache_fetched_at ON page_cache(fetched_at)",
            [],
        )?;
        Ok(Self { pool })
    }

    /// Open an in-memory page cache.
    pub fn in_memory() -> Result<Self, rusqlite::Error> {
        let manager = r2d2_sqlite::SqliteConnectionManager::memory();
        let pool = r2d2::Pool::builder()
            .build(manager)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        let conn = pool
            .get()
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS page_cache (
                url TEXT PRIMARY KEY,
                title TEXT,
                text TEXT NOT NULL,
                fetched_at INTEGER NOT NULL
            )",
            [],
        )?;
        Ok(Self { pool })
    }

    /// Get a cached entry if present and not expired.
    pub fn get(&self, url: &str, max_age_secs: u64) -> Result<Option<CacheEntry>, rusqlite::Error> {
        let conn = self
            .pool
            .get()
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        let cutoff = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64
            - max_age_secs as i64;
        let mut stmt = conn.prepare(
            "SELECT url, title, text, fetched_at FROM page_cache WHERE url = ?1 AND fetched_at > ?2"
        )?;
        let mut rows = stmt.query(rusqlite::params![url, cutoff])?;
        if let Some(row) = rows.next()? {
            Ok(Some(CacheEntry {
                url: row.get(0)?,
                title: row.get(1)?,
                text: row.get(2)?,
                fetched_at: row.get(3)?,
            }))
        } else {
            Ok(None)
        }
    }

    /// Store or replace a cached entry.
    pub fn put(&self, entry: &CacheEntry) -> Result<(), rusqlite::Error> {
        let conn = self
            .pool
            .get()
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        conn.execute(
            "INSERT OR REPLACE INTO page_cache (url, title, text, fetched_at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![entry.url, entry.title, entry.text, entry.fetched_at],
        )?;
        Ok(())
    }

    /// Count cached entries.
    pub fn count(&self) -> Result<u64, rusqlite::Error> {
        let conn = self
            .pool
            .get()
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM page_cache", [], |row| row.get(0))?;
        Ok(count as u64)
    }

    /// Remove entries older than `max_age_secs`.
    pub fn prune(&self, max_age_secs: u64) -> Result<usize, rusqlite::Error> {
        let conn = self
            .pool
            .get()
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        let cutoff = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64
            - max_age_secs as i64;
        let removed = conn.execute("DELETE FROM page_cache WHERE fetched_at < ?1", [cutoff])?;
        Ok(removed)
    }
}
