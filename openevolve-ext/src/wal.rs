//! Write-Ahead Log — append-only JSONL stream of every iteration event.
//!
//! Complements the SQLite `events` table:
//!   - SQLite is the *query* path (indexed lookups, joins with programs).
//!   - WAL is the *replay* path: a single newline-delimited JSON file that
//!     survives any DB corruption and lets us reconstruct a session step by step.
//!
//! Each line is a self-contained `WalEntry` so partial writes at the tail can
//! be detected and discarded on load.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalEntry {
    pub run_id: String,
    pub iteration: u32,
    pub ts: DateTime<Utc>,
    pub op: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    #[serde(default)]
    pub payload: serde_json::Value,
}

pub struct WalWriter {
    file: Mutex<File>,
    path: PathBuf,
}

impl WalWriter {
    /// Open the WAL in append mode, creating parent dirs as needed.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let p = path.as_ref().to_path_buf();
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&p)
            .with_context(|| format!("open WAL {}", p.display()))?;
        Ok(Self {
            file: Mutex::new(file),
            path: p,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Append one entry as a single line. Fsyncs the line so a crash mid-iter
    /// loses at most one event.
    pub fn append(&self, entry: &WalEntry) -> Result<()> {
        let mut line = serde_json::to_string(entry)?;
        line.push('\n');
        let mut f = self.file.lock();
        f.write_all(line.as_bytes())?;
        f.flush()?;
        // Best-effort fsync; on filesystems that don't support it we just skip.
        let _ = f.sync_data();
        Ok(())
    }
}

/// Load all valid entries from a WAL file. Stops cleanly at the first malformed
/// line (typical case: torn write at end-of-file after a crash).
pub fn load(path: impl AsRef<Path>) -> Result<Vec<WalEntry>> {
    let f = File::open(path.as_ref())
        .with_context(|| format!("open WAL for replay {}", path.as_ref().display()))?;
    let r = BufReader::new(f);
    let mut out = Vec::new();
    for line in r.lines() {
        let line = match line {
            Ok(l) if l.trim().is_empty() => continue,
            Ok(l) => l,
            Err(_) => break,
        };
        match serde_json::from_str::<WalEntry>(&line) {
            Ok(e) => out.push(e),
            Err(_) => break, // torn-write tail: stop here
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn append_and_replay_in_order() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("events.jsonl");
        let w = WalWriter::open(&path).unwrap();
        for i in 0..5u32 {
            w.append(&WalEntry {
                run_id: "run1".into(),
                iteration: i,
                ts: Utc::now(),
                op: "mutate".into(),
                score: Some(i as f64 * 0.1),
                payload: serde_json::json!({"i": i}),
            })
            .unwrap();
        }
        drop(w);
        let loaded = load(&path).unwrap();
        assert_eq!(loaded.len(), 5);
        for (i, e) in loaded.iter().enumerate() {
            assert_eq!(e.iteration, i as u32);
        }
    }

    #[test]
    fn torn_write_at_tail_recovers() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("events.jsonl");
        {
            let w = WalWriter::open(&path).unwrap();
            w.append(&WalEntry {
                run_id: "r".into(),
                iteration: 1,
                ts: Utc::now(),
                op: "ok".into(),
                score: None,
                payload: serde_json::json!({}),
            })
            .unwrap();
        }
        // Simulate torn write
        let mut f = OpenOptions::new().append(true).open(&path).unwrap();
        f.write_all(b"{partial\n").unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(loaded.len(), 1);
    }
}
