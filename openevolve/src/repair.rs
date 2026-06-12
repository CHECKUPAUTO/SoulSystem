//! Auto-repair: ask the LLM to fix evolved code that fails to parse or execute.

use crate::llm::LlmClient;
use crate::mutation_engine::extract_code;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tracing::{info, warn};

#[derive(Debug, Clone)]
pub struct RepairConfig {
    pub max_attempts: u32,
    pub temperature: f32,
    pub max_tokens: u32,
}

impl Default for RepairConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            temperature: 0.3,
            max_tokens: 2048,
        }
    }
}

#[derive(Debug, Default)]
pub struct RepairStats {
    pub attempts: AtomicU32,
    pub successes: AtomicU32,
}

impl RepairStats {
    pub fn record_attempt(&self) {
        self.attempts.fetch_add(1, Ordering::Relaxed);
    }
    pub fn record_success(&self) {
        self.successes.fetch_add(1, Ordering::Relaxed);
    }
    pub fn repair_success_rate(&self) -> f64 {
        let a = self.attempts.load(Ordering::Relaxed);
        let s = self.successes.load(Ordering::Relaxed);
        if a == 0 {
            0.0
        } else {
            s as f64 / a as f64
        }
    }
}

pub struct RepairEngine {
    client: LlmClient,
    config: RepairConfig,
    stats: Arc<RepairStats>,
}

impl RepairEngine {
    pub fn new(client: LlmClient, config: RepairConfig) -> Self {
        Self {
            client,
            config,
            stats: Arc::new(RepairStats::default()),
        }
    }

    pub fn stats(&self) -> Arc<RepairStats> {
        Arc::clone(&self.stats)
    }

    /// Try to repair `code`. Returns the fixed code, or `None` if every attempt
    /// returned an empty or unchanged response.
    pub async fn repair(&self, code: &str, language: &str, error_msg: &str) -> Option<String> {
        let mut current = code.to_string();
        for attempt in 1..=self.config.max_attempts {
            self.stats.record_attempt();
            let prompt = format!(
                "The following {language} code has a problem:\n```\n{current}\n```\n\
                 Error message:\n{error_msg}\n\n\
                 Return ONLY the corrected code, with no explanation or commentary."
            );
            match self.client.mutate_with_prompt(&prompt).await {
                Ok(raw) => {
                    let fixed = extract_code(&raw);
                    if !fixed.trim().is_empty() && fixed != current {
                        self.stats.record_success();
                        info!(attempt, "repair succeeded");
                        return Some(fixed);
                    }
                    warn!(attempt, "repair returned same or empty code");
                    if !fixed.trim().is_empty() {
                        current = fixed;
                    }
                }
                Err(e) => warn!(attempt, error=%e, "repair LLM call failed"),
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_basic() {
        let s = RepairStats::default();
        s.record_attempt();
        s.record_attempt();
        s.record_success();
        assert_eq!(s.repair_success_rate(), 0.5);
    }
}
