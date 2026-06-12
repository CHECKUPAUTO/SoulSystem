/// Database client using SQLite via rusqlite.
use rusqlite::Connection;

#[derive(Debug)]
pub struct DatabaseClient {
    conn: Connection,
}

impl DatabaseClient {
    /// Opens a database connection.
    ///
    /// For `:memory:` an in-memory database is created.
    /// For file paths, the file is opened (created if missing).
    ///
    /// # Errors
    /// Returns an error if the connection string is invalid or the database cannot be opened.
    pub fn new(connection_string: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let is_memory =
            connection_string == ":memory:" || connection_string.starts_with("sqlite::memory:");
        let path = if is_memory {
            ":memory:"
        } else {
            connection_string
        };
        let conn = Connection::open(path)?;
        if is_memory {
            // Enable WAL mode for better concurrency in tests
            conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        }
        Ok(Self { conn })
    }

    /// Returns the CREATE TABLE statement(s) for the given table.
    ///
    /// # Errors
    /// Returns an error if the query fails.
    pub fn describe_table(
        &self,
        table_name: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let mut stmt = self
            .conn
            .prepare("SELECT sql FROM sqlite_master WHERE type='table' AND name=?1")?;
        let schema: String = stmt.query_row([table_name], |row| row.get(0))?;
        Ok(schema)
    }
}
