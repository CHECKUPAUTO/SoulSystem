use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub enum CorpusError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("pool: {0}")]
    Pool(#[from] r2d2::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Fingerprint {
    pub ast_hash: String,
    pub structure: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct OriginalityReport {
    pub score: f32,
    pub verdict: String,
    pub closest_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct Submission {
    pub files: std::collections::HashMap<String, String>,
    pub entrypoint: String,
}

pub struct Corpus {
    pool: Pool<SqliteConnectionManager>,
}

impl Corpus {
    pub fn open(db_path: &str) -> Result<Self, CorpusError> {
        let manager = SqliteConnectionManager::file(db_path);
        let pool = Pool::builder().max_size(8).build(manager)?;
        let conn = pool.get()?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             CREATE TABLE IF NOT EXISTS tenants (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 name TEXT NOT NULL UNIQUE,
                 api_key_hash TEXT NOT NULL,
                 created_at INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS submissions (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 tenant_id INTEGER NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
                 label TEXT,
                 fingerprint TEXT NOT NULL,
                 created_at INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_subs_tenant ON submissions(tenant_id);",
        )?;
        Ok(Self { pool })
    }

    pub fn check(
        &self,
        tenant_id: i64,
        submission: &Submission,
        threshold: f32,
    ) -> Result<OriginalityReport, CorpusError> {
        let candidate_fp = fingerprint_submission(submission);
        let candidate_json = serde_json::to_string(&candidate_fp)?;

        let conn = self.pool.get()?;
        let mut stmt =
            conn.prepare("SELECT id, fingerprint FROM submissions WHERE tenant_id = ?1")?;
        let rows = stmt.query_map(params![tenant_id], |row| {
            let id: i64 = row.get(0)?;
            let fp_json: String = row.get(1)?;
            Ok((id, fp_json))
        })?;

        let mut max_score = 0.0_f32;
        let mut closest = None;
        for r in rows {
            let (id, fp_json) = r?;
            let fp: Fingerprint = serde_json::from_str(&fp_json)?;
            let s = similarity(&candidate_fp, &fp);
            if s > max_score {
                max_score = s;
                closest = Some(id);
            }
        }
        let verdict = if max_score > threshold {
            "clone"
        } else {
            "original"
        };
        Ok(OriginalityReport {
            score: max_score,
            verdict: verdict.to_string(),
            closest_id: closest,
        })
    }

    pub fn submit(
        &self,
        tenant_id: i64,
        label: Option<&str>,
        submission: &Submission,
    ) -> Result<i64, CorpusError> {
        let fp = fingerprint_submission(submission);
        let fp_json = serde_json::to_string(&fp)?;
        let conn = self.pool.get()?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        conn.execute(
            "INSERT INTO submissions (tenant_id, label, fingerprint, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![tenant_id, label, fp_json, now],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn tenant_by_api_key(&self, api_key: &str) -> Result<Option<i64>, CorpusError> {
        use sha2::Digest;
        let mut h = sha2::Sha256::new();
        h.update(api_key.as_bytes());
        let hash = hex::encode(h.finalize());
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare("SELECT id FROM tenants WHERE api_key_hash = ?1")?;
        let mut rows = stmt.query(params![hash])?;
        Ok(rows.next()?.map(|row| row.get(0)).transpose()?)
    }
}

fn fingerprint_submission(sub: &Submission) -> Fingerprint {
    use sha2::Digest;
    let mut lines: Vec<&str> = sub.files.values().flat_map(|s| s.lines()).collect();
    lines.sort();
    let structure = serde_json::json!({
        "file_count": sub.files.len(),
        "total_lines": sub.files.values().map(|s| s.lines().count()).sum::<usize>(),
        "entrypoint": sub.entrypoint,
    });

    let mut h = sha2::Sha256::new();
    for l in &lines {
        h.update(l.as_bytes());
    }
    Fingerprint {
        ast_hash: hex::encode(h.finalize()),
        structure,
    }
}

fn similarity(a: &Fingerprint, b: &Fingerprint) -> f32 {
    if a.ast_hash == b.ast_hash {
        return 1.0;
    }
    let mut matches = 0;
    let a_chars: Vec<char> = a.ast_hash.chars().collect();
    let b_chars: Vec<char> = b.ast_hash.chars().collect();
    for i in 0..a_chars.len().min(b_chars.len()) {
        if a_chars[i] == b_chars[i] {
            matches += 1;
        }
    }
    matches as f32 / a_chars.len().max(b_chars.len()) as f32
}
