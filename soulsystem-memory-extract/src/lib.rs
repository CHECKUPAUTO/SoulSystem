use chrono::{DateTime, Utc};
use std::path::{Path, PathBuf};
use std::collections::HashMap;

pub struct ExtractedFact {
    pub text: String,
    pub category: FactCategory,
    pub importance: f64,
    pub source_file: String,
}

pub enum FactCategory { Decision, Architecture, Error, Lesson, Pattern, Service, Project }

pub struct SessionExtractor {
    workspace: PathBuf,
    memory_file: PathBuf,
    wiki_dir: PathBuf,
    daily_dir: PathBuf,
}

impl SessionExtractor {
    pub fn new(workspace: &Path) -> Self {
        Self {
            workspace: workspace.to_path_buf(),
            memory_file: workspace.join("MEMORY.md"),
            wiki_dir: workspace.join("wiki"),
            daily_dir: workspace.join("memory"),
        }
    }

    pub fn extract(&self) -> anyhow::Result<ExtractionReport> {
        let mut facts = Vec::new();
        if self.daily_dir.exists() {
            for entry in std::fs::read_dir(&self.daily_dir)? {
                let path = entry?.path();
                if path.extension().and_then(|s| s.to_str()) != Some("md") { continue; }
                let content = std::fs::read_to_string(&path)?;
                facts.extend(Self::extract_from_text(&content, path.to_string_lossy().to_string()));
            }
        }
        facts.sort_by(|a, b| b.importance.partial_cmp(&a.importance).unwrap_or(std::cmp::Ordering::Equal));
        Ok(ExtractionReport { facts, memory_size: std::fs::read_to_string(&self.memory_file).unwrap_or_default().len() })
    }

    fn extract_from_text(text: &str, source: String) -> Vec<ExtractedFact> {
        let mut facts = Vec::new();
        for line in text.lines() {
            for keyword in &["Décision", "Architecture", "Migration", "Installé", "BUG", "Erreur", "Leçon"] {
                if line.contains(keyword) {
                    facts.push(ExtractedFact {
                        text: line.to_string(),
                        category: match *keyword { "BUG" | "Erreur" => FactCategory::Error, "Décision" => FactCategory::Decision, "Architecture" => FactCategory::Architecture, "Leçon" => FactCategory::Lesson, _ => FactCategory::Pattern },
                        importance: 0.5,
                        source_file: source.clone(),
                    });
                }
            }
        }
        facts
    }
}

pub struct ExtractionReport {
    pub facts: Vec<ExtractedFact>,
    pub memory_size: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    #[test] fn test_extract_finds_decision() {
        let tmp = TempDir::new().unwrap();
        let extractor = SessionExtractor::new(tmp.path());
        let facts = SessionExtractor::extract_from_text("Décision: utiliser Rust", "test.md");
        assert!(facts.iter().any(|f| f.text.contains("Décision")));
    }
}
