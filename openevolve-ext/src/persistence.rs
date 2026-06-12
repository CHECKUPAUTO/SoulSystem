//! Persistent storage for programs, runs and metrics.
//!
//! Schema:
//!   - `runs`     : a single evolution session (id, started_at, config snapshot, status)
//!   - `programs` : every program ever evaluated (id, run_id, code, language, score,
//!     metrics_json, embedding_blob, parent_id, generation, island,
//!     created_at, provenance)
//!   - `events`   : iteration WAL stream (id, run_id, iteration, ts, op, score, payload_json)
//!
//! Uses `rusqlite` with the `bundled` feature (no system libsqlite3 dep) and
//! `r2d2` connection pooling so async tasks share the same database without
//! blocking each other.

use crate::program::Program;
use anyhow::{Context, Result};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::info;

pub type SqlitePool = Pool<SqliteConnectionManager>;

/// Open (or create) a SQLite database at `path`, apply schema migrations,
/// and return a connection pool sized for the current parallelism.
pub fn open_pool(path: &Path) -> Result<SqlitePool> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let manager = SqliteConnectionManager::file(path).with_init(|c| {
        c.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;",
        )
    });
    let pool = Pool::builder()
        .max_size(num_cpus::get().max(4) as u32)
        .build(manager)
        .context("build sqlite pool")?;
    migrate(&pool)?;
    info!("SQLite ready at {}", path.display());
    Ok(pool)
}

/// Apply (idempotent) schema migrations.
pub fn migrate(pool: &SqlitePool) -> Result<()> {
    let conn = pool.get()?;
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS runs (
            id          TEXT PRIMARY KEY,
            started_at  TEXT NOT NULL,
            finished_at TEXT,
            status      TEXT NOT NULL,
            config_json TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS programs (
            id           TEXT PRIMARY KEY,
            run_id       TEXT NOT NULL,
            code         TEXT NOT NULL,
            language     TEXT NOT NULL,
            score        REAL NOT NULL,
            generation   INTEGER NOT NULL,
            parent_id    TEXT,
            island       INTEGER NOT NULL,
            created_at   TEXT NOT NULL,
            metrics_json TEXT NOT NULL,
            provenance   TEXT NOT NULL,
            embedding    BLOB,
            FOREIGN KEY (run_id) REFERENCES runs(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_programs_run_score ON programs(run_id, score DESC);
        CREATE INDEX IF NOT EXISTS idx_programs_run_island ON programs(run_id, island);
        CREATE INDEX IF NOT EXISTS idx_programs_parent ON programs(parent_id);

        CREATE TABLE IF NOT EXISTS events (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            run_id      TEXT NOT NULL,
            iteration   INTEGER NOT NULL,
            ts          TEXT NOT NULL,
            op          TEXT NOT NULL,
            score       REAL,
            payload     TEXT NOT NULL,
            FOREIGN KEY (run_id) REFERENCES runs(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_events_run_iter ON events(run_id, iteration);

        CREATE TABLE IF NOT EXISTS costs (
            id        INTEGER PRIMARY KEY AUTOINCREMENT,
            run_id    TEXT NOT NULL,
            provider  TEXT NOT NULL,
            model     TEXT NOT NULL,
            ts        TEXT NOT NULL,
            tokens_in INTEGER NOT NULL,
            tokens_out INTEGER NOT NULL,
            usd_estimate REAL NOT NULL,
            FOREIGN KEY (run_id) REFERENCES runs(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_costs_run ON costs(run_id);
        "#,
    )?;
    Ok(())
}

// ── Runs ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunRow {
    pub id: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub status: String,
    pub config_json: String,
}

pub fn create_run(pool: &SqlitePool, run_id: &str, config_json: &str) -> Result<()> {
    pool.get()?.execute(
        "INSERT OR REPLACE INTO runs (id, started_at, status, config_json) VALUES (?1, ?2, ?3, ?4)",
        params![
            run_id,
            chrono::Utc::now().to_rfc3339(),
            "running",
            config_json
        ],
    )?;
    Ok(())
}

pub fn finish_run(pool: &SqlitePool, run_id: &str, status: &str) -> Result<()> {
    pool.get()?.execute(
        "UPDATE runs SET finished_at = ?1, status = ?2 WHERE id = ?3",
        params![chrono::Utc::now().to_rfc3339(), status, run_id],
    )?;
    Ok(())
}

pub fn list_runs(pool: &SqlitePool, limit: usize) -> Result<Vec<RunRow>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT id, started_at, finished_at, status, config_json
         FROM runs ORDER BY started_at DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit as i64], |r| {
        Ok(RunRow {
            id: r.get(0)?,
            started_at: r.get(1)?,
            finished_at: r.get(2)?,
            status: r.get(3)?,
            config_json: r.get(4)?,
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

// ── Programs ────────────────────────────────────────────────────────────

pub fn insert_program(
    pool: &SqlitePool,
    run_id: &str,
    p: &Program,
    embedding: Option<&[f32]>,
) -> Result<()> {
    let metrics_json = serde_json::to_string(&p.metrics).unwrap_or_else(|_| "{}".into());
    let provenance = p
        .provenance
        .as_ref()
        .cloned()
        .unwrap_or_else(|| "unknown".into());
    let blob = embedding.map(embedding_to_bytes);
    pool.get()?.execute(
        "INSERT OR REPLACE INTO programs
         (id, run_id, code, language, score, generation, parent_id, island,
          created_at, metrics_json, provenance, embedding)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            p.id,
            run_id,
            p.code,
            p.language.as_deref().unwrap_or(""),
            p.score,
            p.generation as i64,
            p.parent_id,
            p.island_id as i64,
            p.created_at.to_rfc3339(),
            metrics_json,
            provenance,
            blob,
        ],
    )?;
    Ok(())
}

pub fn best_program(pool: &SqlitePool, run_id: &str) -> Result<Option<Program>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT id, code, language, score, generation, parent_id, island,
                created_at, metrics_json, provenance
         FROM programs WHERE run_id = ?1
         ORDER BY score DESC, generation ASC LIMIT 1",
    )?;
    let mut rows = stmt.query(params![run_id])?;
    match rows.next()? {
        Some(r) => Ok(Some(row_to_program(r)?)),
        None => Ok(None),
    }
}

pub fn count_programs(pool: &SqlitePool, run_id: &str) -> Result<i64> {
    let conn = pool.get()?;
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM programs WHERE run_id = ?1",
        params![run_id],
        |r| r.get(0),
    )?;
    Ok(n)
}

pub fn top_programs(pool: &SqlitePool, run_id: &str, limit: usize) -> Result<Vec<Program>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT id, code, language, score, generation, parent_id, island,
                created_at, metrics_json, provenance
         FROM programs WHERE run_id = ?1
         ORDER BY score DESC LIMIT ?2",
    )?;
    let mut rows = stmt.query(params![run_id, limit as i64])?;
    let mut out = Vec::new();
    while let Some(r) = rows.next()? {
        if let Ok(p) = row_to_program(r) {
            out.push(p);
        }
    }
    Ok(out)
}

pub fn programs_on_island(pool: &SqlitePool, run_id: &str, island: usize) -> Result<Vec<Program>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT id, code, language, score, generation, parent_id, island,
                created_at, metrics_json, provenance
         FROM programs WHERE run_id = ?1 AND island = ?2
         ORDER BY score DESC",
    )?;
    let mut rows = stmt.query(params![run_id, island as i64])?;
    let mut out = Vec::new();
    while let Some(r) = rows.next()? {
        if let Ok(p) = row_to_program(r) {
            out.push(p);
        }
    }
    Ok(out)
}

pub fn fetch_embedding(pool: &SqlitePool, program_id: &str) -> Result<Option<Vec<f32>>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare("SELECT embedding FROM programs WHERE id = ?1")?;
    let mut rows = stmt.query(params![program_id])?;
    if let Some(r) = rows.next()? {
        let blob: Option<Vec<u8>> = r.get(0)?;
        return Ok(blob.map(|b| bytes_to_embedding(&b)));
    }
    Ok(None)
}

pub fn similar_programs(
    pool: &SqlitePool,
    run_id: &str,
    target: &[f32],
    limit: usize,
) -> Result<Vec<(Program, f32)>> {
    // For now: pull recent embeddings into memory and compute cosine.
    // For very large bases use a proper vector index — but at our scale, fine.
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT id, code, language, score, generation, parent_id, island,
                created_at, metrics_json, provenance, embedding
         FROM programs WHERE run_id = ?1 AND embedding IS NOT NULL
         ORDER BY created_at DESC LIMIT 500",
    )?;
    let mut rows = stmt.query(params![run_id])?;
    let mut scored: Vec<(Program, f32)> = Vec::new();
    while let Some(r) = rows.next()? {
        let blob: Vec<u8> = r.get(10)?;
        let e = bytes_to_embedding(&blob);
        let s = cosine(target, &e);
        let p = row_to_program(r)?;
        scored.push((p, s));
    }
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(limit);
    Ok(scored)
}

// ── Events ──────────────────────────────────────────────────────────────

pub fn record_event(
    pool: &SqlitePool,
    run_id: &str,
    iteration: u32,
    op: &str,
    score: Option<f64>,
    payload: &serde_json::Value,
) -> Result<()> {
    pool.get()?.execute(
        "INSERT INTO events (run_id, iteration, ts, op, score, payload)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            run_id,
            iteration as i64,
            chrono::Utc::now().to_rfc3339(),
            op,
            score,
            payload.to_string(),
        ],
    )?;
    Ok(())
}

// ── Costs ───────────────────────────────────────────────────────────────

pub fn record_cost(
    pool: &SqlitePool,
    run_id: &str,
    provider: &str,
    model: &str,
    tokens_in: u64,
    tokens_out: u64,
    usd: f64,
) -> Result<()> {
    pool.get()?.execute(
        "INSERT INTO costs (run_id, provider, model, ts, tokens_in, tokens_out, usd_estimate)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            run_id,
            provider,
            model,
            chrono::Utc::now().to_rfc3339(),
            tokens_in as i64,
            tokens_out as i64,
            usd,
        ],
    )?;
    Ok(())
}

pub fn run_total_cost(pool: &SqlitePool, run_id: &str) -> Result<(u64, u64, f64)> {
    let conn = pool.get()?;
    let row = conn.query_row(
        "SELECT COALESCE(SUM(tokens_in),0), COALESCE(SUM(tokens_out),0), COALESCE(SUM(usd_estimate),0.0)
         FROM costs WHERE run_id = ?1",
        params![run_id],
        |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, f64>(2)?)),
    )?;
    Ok((row.0 as u64, row.1 as u64, row.2))
}

// ── Helpers ─────────────────────────────────────────────────────────────

fn row_to_program(r: &rusqlite::Row) -> Result<Program> {
    let id: String = r.get(0)?;
    let code: String = r.get(1)?;
    let language: String = r.get(2)?;
    let score: f64 = r.get(3)?;
    let generation: i64 = r.get(4)?;
    let parent_id: Option<String> = r.get(5)?;
    let island: i64 = r.get(6)?;
    let created_at: String = r.get(7)?;
    let metrics_json: String = r.get(8)?;
    let provenance: String = r.get(9)?;
    let metrics: std::collections::HashMap<String, f64> =
        serde_json::from_str(&metrics_json).unwrap_or_default();
    let ts = chrono::DateTime::parse_from_rfc3339(&created_at)
        .map(|t| t.with_timezone(&chrono::Utc))
        .unwrap_or_else(|_| chrono::Utc::now());
    Ok(Program {
        id,
        code,
        language: if language.is_empty() {
            None
        } else {
            Some(language)
        },
        score,
        generation: generation as u32,
        parent_id,
        island_id: island as usize,
        created_at: ts,
        metrics,
        provenance: if provenance.is_empty() {
            None
        } else {
            Some(provenance)
        },
    })
}

fn embedding_to_bytes(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for x in v {
        out.extend_from_slice(&x.to_le_bytes());
    }
    out
}

fn bytes_to_embedding(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.is_empty() || a.len() != b.len() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

// Tiny num_cpus polyfill so we don't need the crate
mod num_cpus {
    pub fn get() -> usize {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn fresh_pool() -> (tempfile::TempDir, SqlitePool) {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.db");
        let pool = open_pool(&path).unwrap();
        (dir, pool)
    }

    #[test]
    fn migrations_idempotent() {
        let (_d, pool) = fresh_pool();
        migrate(&pool).unwrap();
        migrate(&pool).unwrap(); // second call must not fail
    }

    #[test]
    fn roundtrip_program() {
        let (_d, pool) = fresh_pool();
        create_run(&pool, "r1", "{}").unwrap();
        let p = Program::new("fn x(){}".into(), 0, None, 0);
        insert_program(&pool, "r1", &p, Some(&[0.1f32; 16])).unwrap();
        let n = count_programs(&pool, "r1").unwrap();
        assert_eq!(n, 1);
        let top = top_programs(&pool, "r1", 10).unwrap();
        assert_eq!(top[0].id, p.id);
        let emb = fetch_embedding(&pool, &p.id).unwrap().unwrap();
        assert_eq!(emb.len(), 16);
    }

    #[test]
    fn best_returns_highest_score() {
        let (_d, pool) = fresh_pool();
        create_run(&pool, "r1", "{}").unwrap();
        let mut a = Program::new("a".into(), 0, None, 0);
        a.score = 0.3;
        let mut b = Program::new("b".into(), 1, None, 0);
        b.score = 0.9;
        let mut c = Program::new("c".into(), 2, None, 0);
        c.score = 0.5;
        insert_program(&pool, "r1", &a, None).unwrap();
        insert_program(&pool, "r1", &b, None).unwrap();
        insert_program(&pool, "r1", &c, None).unwrap();
        let best = best_program(&pool, "r1").unwrap().unwrap();
        assert_eq!(best.id, b.id);
    }

    #[test]
    fn events_and_costs() {
        let (_d, pool) = fresh_pool();
        create_run(&pool, "r1", "{}").unwrap();
        record_event(
            &pool,
            "r1",
            42,
            "mutate",
            Some(0.5),
            &serde_json::json!({"a": 1}),
        )
        .unwrap();
        record_cost(&pool, "r1", "openai", "gpt-4", 100, 50, 0.003).unwrap();
        let (ti, to, usd) = run_total_cost(&pool, "r1").unwrap();
        assert_eq!(ti, 100);
        assert_eq!(to, 50);
        assert!((usd - 0.003).abs() < 1e-9);
    }
}
