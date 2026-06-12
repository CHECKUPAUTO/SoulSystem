//! Minimal database schema initialization for SQLite persistence.

/// Stats grouped by category or action.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct GroupStats {
    pub n: u64,
    pub win_rate: f64,
    pub mean_return_bps: f64,
    pub median_return_bps: f64,
    pub std_return_bps: f64,
    pub stop_loss_hit_rate: f64,
}

use rusqlite::Connection;

/// Initialize the SQLite database schema. Idempotent.
pub fn init(conn: &mut Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS decisions (
            id          TEXT PRIMARY KEY,
            schema_id   TEXT NOT NULL,
            action      INTEGER NOT NULL,
            timestamp   INTEGER NOT NULL,
            details     TEXT
        );
        CREATE TABLE IF NOT EXISTS performance (
            id          TEXT PRIMARY KEY,
            schema_id   TEXT NOT NULL,
            pnl         REAL NOT NULL,
            sharpe      REAL,
            drawdown    REAL,
            timestamp   INTEGER NOT NULL
        );",
    )?;
    Ok(())
}
