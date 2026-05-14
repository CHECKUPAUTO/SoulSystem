use std::path::{Path, PathBuf};

pub struct ExtractedFact {
    pub text: String,
    pub category: FactCategory,
    pub importance: f64,
    pub source_file: String,
}

pub enum FactCategory { Decision, Architecture, Error, Lesson, Pattern, Service, Project }

pub struct SessionExtractor {
    memory_file: PathBuf,
    wiki_dir: PathBuf,
    daily_dir: PathBuf,
}

impl SessionExtractor {
    pub fn new(workspace: &Path) -> Self {
        Self {
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
        let report = ExtractionReport { facts, memory_size: std::fs::read_to_string(&self.memory_file).unwrap_or_default().len() };

        self.persist_results(&report)?;

        Ok(report)
    }

    fn persist_results(&self, report: &ExtractionReport) -> anyhow::Result<()> {
        use std::io::Write;

        // Persist to MEMORY.md
        let mut mem_file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.memory_file)?;

        for fact in &report.facts {
            if fact.importance >= 0.7 {
                writeln!(mem_file, "- [EXTRACTED] {} (Source: {})", fact.text, fact.source_file)?;
            }
        }

        // Persist Patterns to wiki/
        if !self.wiki_dir.exists() {
            std::fs::create_dir_all(&self.wiki_dir)?;
        }

        for fact in &report.facts {
            if matches!(fact.category, FactCategory::Pattern) {
                let wiki_file = self.wiki_dir.join("patterns.md");
                let mut f = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(wiki_file)?;
                writeln!(f, "- Pattern identified: {} (Importance: {})", fact.text, fact.importance)?;
            }
        }

        Ok(())
    }

    fn extract_from_text(text: &str, source: String) -> Vec<ExtractedFact> {
        use regex::Regex;
        let mut facts = Vec::new();

        let patterns = [
            (Regex::new(r"(?i)Décision:\s*(?P<val>.*)").unwrap(), FactCategory::Decision, 0.8),
            (Regex::new(r"(?i)Architecture:\s*(?P<val>.*)").unwrap(), FactCategory::Architecture, 0.9),
            (Regex::new(r"(?i)BUG|Erreur:\s*(?P<val>.*)").unwrap(), FactCategory::Error, 0.7),
            (Regex::new(r"(?i)Leçon:\s*(?P<val>.*)").unwrap(), FactCategory::Lesson, 0.6),
            (Regex::new(r"(?i)Pattern:\s*(?P<val>.*)").unwrap(), FactCategory::Pattern, 0.5),
            (Regex::new(r"(?i)Service:\s*(?P<val>.*)").unwrap(), FactCategory::Service, 0.4),
            (Regex::new(r"(?i)Project:\s*(?P<val>.*)").unwrap(), FactCategory::Project, 0.3),
        ];

        for line in text.lines() {
            for (re, category, importance) in &patterns {
                if let Some(caps) = re.captures(line) {
                    facts.push(ExtractedFact {
                        text: caps["val"].trim().to_string(),
                        category: *category,
                        importance: *importance,
                        source_file: source.clone(),
                    });
                }
            }
        }
        facts
    }
}

impl Clone for FactCategory {
    fn clone(&self) -> Self {
        *self
    }
}

impl Copy for FactCategory {}

pub struct ExtractionReport {
    pub facts: Vec<ExtractedFact>,
    pub memory_size: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_extract_finds_decision() {
        let facts = SessionExtractor::extract_from_text("Décision: utiliser Rust", "test.md".into());
        assert!(facts.iter().any(|f| f.text == "utiliser Rust" && matches!(f.category, FactCategory::Decision)));
    }

    #[test]
    fn test_extract_persistence() {
        let tmp = TempDir::new().unwrap();
        let daily = tmp.path().join("memory");
        std::fs::create_dir(&daily).unwrap();
        std::fs::write(daily.join("log1.md"), "Architecture: Microservices\nPattern: Circuit Breaker\nDécision: Use Axum").unwrap();

        let extractor = SessionExtractor::new(tmp.path());
        let report = extractor.extract().unwrap();

        assert_eq!(report.facts.len(), 3);

        // Check MEMORY.md (importance >= 0.7)
        let memory_content = std::fs::read_to_string(tmp.path().join("MEMORY.md")).unwrap();
        assert!(memory_content.contains("Microservices"));
        assert!(memory_content.contains("Use Axum"));
        assert!(!memory_content.contains("Circuit Breaker")); // importance 0.5 < 0.7

        // Check wiki/patterns.md
        let patterns_content = std::fs::read_to_string(tmp.path().join("wiki/patterns.md")).unwrap();
        assert!(patterns_content.contains("Circuit Breaker"));
    }
}
