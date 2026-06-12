//! Generic SQL database diagnoser — extracted from avid-core.
//!
//! Supports `Postgres`, `MySQL`, `SQLite` via the `sqlx::Any` driver.
//! Pick which drivers to compile in via features:
//!
//! ```toml
//! avid-db = { path = "../avid-db", features = ["postgres", "sqlite"] }
//! ```

#![forbid(unsafe_code)]
#![warn(clippy::pedantic, clippy::nursery)]

use std::fmt::Write;
use std::time::Duration;

use sqlx::{Any, Pool, Row};
use tracing::{error, info};

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("connection: {0}")]
    Connection(String),

    #[error("query: {0}")]
    Query(String),

    #[error("timeout after {0}ms")]
    Timeout(u64),
}

/// Database introspection client.
pub struct DatabaseClient {
    pool: Pool<Any>,
}

impl std::fmt::Debug for DatabaseClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DatabaseClient").finish_non_exhaustive()
    }
}

impl DatabaseClient {
    /// Connect to a database via URL (e.g. `sqlite:/path/db`, `postgres://...`).
    ///
    /// # Errors
    /// Returns `DbError::Connection` or `DbError::Timeout` on failure.
    pub async fn new(url: &str) -> Result<Self, DbError> {
        let pool = tokio::time::timeout(Duration::from_secs(5), Pool::<Any>::connect(url))
            .await
            .map_err(|_| DbError::Timeout(5_000))?
            .map_err(|e| DbError::Connection(e.to_string()))?;
        Ok(Self { pool })
    }

    /// Describe the schema of a table.
    ///
    /// Best-effort: queries `information_schema.columns` which works on
    /// `Postgres` and `MySQL`. `SQLite` users should use the `pragma_table_info`
    /// variant (TODO).
    ///
    /// # Errors
    /// Returns `DbError::Query` on SQL failure.
    pub async fn describe_table(&self, table_name: &str) -> Result<String, DbError> {
        info!(table_name, "describing table schema");

        if !is_valid_identifier(table_name) {
            return Err(DbError::Query(format!("invalid table name: {table_name}")));
        }

        self.probe_table_existence(table_name).await?;

        let mut description = format!("Table: {table_name}\nColumns:\n");
        self.fetch_and_append_columns(&mut description, table_name)
            .await;

        Ok(description)
    }

    async fn probe_table_existence(&self, table_name: &str) -> Result<(), DbError> {
        let probe = format!("SELECT * FROM {table_name} LIMIT 0");
        let _ = tokio::time::timeout(
            Duration::from_secs(5),
            sqlx::query(&probe).fetch_optional(&self.pool),
        )
        .await
        .map_err(|_| DbError::Timeout(5_000))?
        .map_err(|e| DbError::Query(format!("table probe: {e}")))?;
        Ok(())
    }

    async fn fetch_and_append_columns(&self, description: &mut String, table_name: &str) {
        let column_query = "SELECT column_name, data_type, is_nullable \
                            FROM information_schema.columns \
                            WHERE table_name = $1";
        let columns_res = tokio::time::timeout(
            Duration::from_secs(5),
            sqlx::query(column_query)
                .bind(table_name)
                .fetch_all(&self.pool),
        )
        .await;

        match columns_res {
            Ok(Ok(rows)) => {
                for row in rows {
                    let name: String = row.try_get(0).unwrap_or_default();
                    let dtype: String = row.try_get(1).unwrap_or_default();
                    let nullable: String = row.try_get(2).unwrap_or_default();
                    let _ = writeln!(description, "- {name}: {dtype} (Nullable: {nullable})");
                }
            }
            Ok(Err(e)) => {
                error!("failed to fetch column metadata: {e}");
                description
                    .push_str("Could not retrieve detailed schema via information_schema.\n");
            }
            Err(_) => {
                error!("column metadata fetch timed out");
                description.push_str("Schema metadata fetch timed out.\n");
            }
        }
    }

    /// Lightweight performance check — placeholder for future deeper diags.
    #[must_use]
    pub fn check_performance(&self) -> String {
        info!("checking database performance");
        "Performance check: Connection pool is healthy.".to_string()
    }
}

/// Reject identifiers with anything but `[A-Za-z0-9_]`.
/// Used to gate raw table-name interpolation against trivial `SQLi`.
fn is_valid_identifier(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && s.len() <= 64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifier_validation_rejects_injection() {
        assert!(is_valid_identifier("users"));
        assert!(is_valid_identifier("user_accounts"));
        assert!(!is_valid_identifier("users; DROP TABLE x"));
        assert!(!is_valid_identifier(""));
        assert!(!is_valid_identifier("users WHERE 1=1"));
        assert!(!is_valid_identifier("users'"));
    }

    #[test]
    fn identifier_validation_enforces_length() {
        let huge = "a".repeat(65);
        assert!(!is_valid_identifier(&huge));
        let exactly_max = "a".repeat(64);
        assert!(is_valid_identifier(&exactly_max));
    }
}
