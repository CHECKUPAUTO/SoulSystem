//! Transfer learning across tasks using embedding similarity.

use anyhow::Result;
use tracing::info;

use crate::program::Program;

pub struct TransferEngine;

impl Default for TransferEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl TransferEngine {
    pub fn new() -> Self {
        Self
    }

    /// Extract reusable patterns from a successful program.
    pub fn extract_patterns(&self, program: &Program) -> Vec<String> {
        let mut patterns = Vec::new();
        for line in program.code.lines() {
            let t = line.trim();
            if t.starts_with("fn ")
                || t.starts_with("pub fn ")
                || t.starts_with("use ")
                || t.starts_with("pub use ")
            {
                patterns.push(t.to_string());
            }
        }
        patterns
    }

    /// Find reusable patterns from history by name-based similarity.
    /// In v4, reads saved programs from the output directory.
    pub fn find_reusable_patterns(
        &self,
        _code: &str,
        _language: &str,
        limit: usize,
    ) -> Result<Vec<String>> {
        let prog_dir = std::path::Path::new("output/programs");
        if !prog_dir.exists() {
            return Ok(Vec::new());
        }

        let mut candidates = Vec::new();
        if let Ok(entries) = std::fs::read_dir(prog_dir) {
            for entry in entries.flatten() {
                if entry.path().extension().is_some_and(|e| e == "json") {
                    if let Ok(content) = std::fs::read_to_string(entry.path()) {
                        if let Ok(prog) = serde_json::from_str::<Program>(&content) {
                            if prog.score > 0.5 {
                                candidates.push(prog);
                            }
                        }
                    }
                }
            }
        }

        candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        let mut patterns = Vec::new();
        for prog in candidates.iter().take(limit) {
            patterns.extend(self.extract_patterns(prog));
        }

        info!("Found {} reusable patterns from history", patterns.len());
        Ok(patterns)
    }

    /// Build an initial prompt enriched with transferred patterns.
    pub fn build_prompt_with_transfer(
        &self,
        base_prompt: &str,
        code: &str,
        language: &str,
        limit: usize,
    ) -> Result<String> {
        let patterns = self.find_reusable_patterns(code, language, limit)?;
        if patterns.is_empty() {
            return Ok(base_prompt.to_string());
        }
        let block = patterns.join("\n");
        Ok(format!(
            "{base_prompt}\n\n## Successful patterns from similar tasks:\n```\n{block}\n```\n"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_patterns_detects_fns() {
        let engine = TransferEngine::new();
        let prog = Program::new("fn solve(x: i32) -> i32 { x * 2 }".into(), 0, None, 0);
        let pats = engine.extract_patterns(&prog);
        assert!(pats.iter().any(|p| p.contains("fn solve")));
    }

    #[test]
    fn test_build_prompt_empty_patterns() {
        let engine = TransferEngine::new();
        let prompt = engine
            .build_prompt_with_transfer("Solve this", "x=1", "rust", 5)
            .unwrap();
        assert!(prompt.contains("Solve this"));
    }
}
