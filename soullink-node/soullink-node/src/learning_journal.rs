//! LearningJournal — persistent append-only JSONL memory.
//!
//! Records key events to a file that survives process restarts.
//! New sessions append to the same file, building a long-term learning trace.
//!
//! Categories:
//! - `intervention` — meta-cortex action outcomes
//! - `mutation`     — evolution mutation applied + fitness delta
//! - `milestone`    — fitness peak or new best
//! - `insight`      — meta-cortex analysis (stuck, loop detected)
//! - `claude`       — Claude bridge proposals & outcomes
//! - `heal`         — auto-heal events
//! - `death`        — death avoidance / rebirth events

use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use serde::Serialize;

use crate::brain::now_f64;

// ── Entry ──

#[derive(Debug, Clone, Serialize)]
pub struct JournalEntry {
    pub ts: f64,
    pub tick: u64,
    pub category: String,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fitness: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

// ── Journal ──

#[derive(Debug)]
pub struct LearningJournal {
    writer: BufWriter<File>,
    buffered: usize,
    flush_every: usize,
}

impl LearningJournal {
    pub fn open(path: &str) -> Result<Self, String> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| format!("Failed to open journal '{}': {}", path, e))?;
        Ok(Self {
            writer: BufWriter::new(file),
            buffered: 0,
            flush_every: 10,
        })
    }

    pub fn record(
        &mut self,
        tick: u64,
        category: &str,
        summary: &str,
        fitness: Option<f64>,
        data: Option<serde_json::Value>,
    ) {
        let entry = JournalEntry {
            ts: now_f64(),
            tick,
            category: category.to_string(),
            summary: summary.to_string(),
            fitness,
            data,
        };
        match serde_json::to_string(&entry) {
            Ok(line) => {
                let _ = writeln!(self.writer, "{}", line);
                self.buffered += 1;
                if self.buffered >= self.flush_every {
                    self.flush();
                }
            }
            Err(e) => {
                tracing::error!("LearningJournal: serialize failed: {}", e);
            }
        }
    }

    pub fn flush(&mut self) {
        let _ = self.writer.flush();
        self.buffered = 0;
    }
}

impl Drop for LearningJournal {
    fn drop(&mut self) {
        self.flush();
    }
}
