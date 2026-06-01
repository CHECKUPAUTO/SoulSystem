//! Adaptive prompt memory — tracks which mutation prompts produced the best results.

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptRecord {
    pub prompt_text: String,
    pub language: String,
    pub avg_score: f64,
    pub usage_count: u32,
}

#[derive(Debug, Clone, Default)]
pub struct PromptMemory {
    records: Arc<Mutex<HashMap<String, PromptRecord>>>,
}

impl PromptMemory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&self, prompt: &str, language: &str, score: f64) {
        let key = key_for(language, prompt);
        let mut m = self.records.lock();
        let entry = m.entry(key).or_insert_with(|| PromptRecord {
            prompt_text: prompt.to_string(),
            language: language.to_string(),
            avg_score: 0.0,
            usage_count: 0,
        });
        let n = entry.usage_count as f64;
        entry.avg_score = (entry.avg_score * n + score) / (n + 1.0);
        entry.usage_count += 1;
    }

    /// Pick the best historically-known prompt for this language, with an extra
    /// bonus when the prompt mentions the requested complexity class. Falls
    /// back to `default` if no recorded prompt clears `min_avg_score`.
    pub fn select_prompt(
        &self,
        language: &str,
        complexity_class: &str,
        min_avg_score: f64,
        default: &str,
    ) -> String {
        let m = self.records.lock();
        let mut best: Option<(&PromptRecord, f64)> = None;
        for r in m.values() {
            if r.language != language {
                continue;
            }
            let bonus = if r.prompt_text.contains(complexity_class) {
                0.05
            } else {
                0.0
            };
            let adj = r.avg_score + bonus;
            match best {
                None => best = Some((r, adj)),
                Some((_, s)) if adj > s => best = Some((r, adj)),
                _ => {}
            }
        }
        match best {
            Some((r, _)) if r.avg_score >= min_avg_score => {
                info!(
                    avg_score = r.avg_score,
                    uses = r.usage_count,
                    "selected memorised prompt"
                );
                r.prompt_text.clone()
            }
            _ => default.to_string(),
        }
    }

    pub fn success_rate(&self, language: &str) -> f64 {
        let m = self.records.lock();
        let mut total = 0.0;
        let mut count = 0u32;
        for r in m.values() {
            if r.language == language {
                total += r.avg_score * r.usage_count as f64;
                count += r.usage_count;
            }
        }
        if count == 0 {
            0.0
        } else {
            total / count as f64
        }
    }

    pub fn snapshot(&self) -> Vec<PromptRecord> {
        self.records.lock().values().cloned().collect()
    }
}

fn key_for(language: &str, prompt: &str) -> String {
    let mut h = Sha256::new();
    h.update(prompt.as_bytes());
    format!("{language}::{}", hex::encode(&h.finalize()[..8]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_and_select() {
        let pm = PromptMemory::new();
        pm.record("Optimize for speed", "python", 0.8);
        pm.record("Optimize for speed", "python", 0.9);
        let p = pm.select_prompt("python", "O(n)", 0.5, "default");
        assert_eq!(p, "Optimize for speed");
        assert!(pm.success_rate("python") > 0.7);
    }

    #[test]
    fn fallback_when_below_threshold() {
        let pm = PromptMemory::new();
        pm.record("weak", "rust", 0.1);
        let p = pm.select_prompt("rust", "O(1)", 0.5, "default");
        assert_eq!(p, "default");
    }
}
