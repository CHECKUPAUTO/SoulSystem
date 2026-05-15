/// Robust regex-based fact extractor for session logs and memory files.
///
/// Uses 7 curated regex patterns to capture facts, preferences, decisions,
/// patterns, todos, goals, and lessons from Markdown files. Results are
/// deduplicated via DashSet and sorted by confidence (total_cmp).
use crate::nlp::refine_category;
use crate::MemoryExtractConfig;

use anyhow::{Context, Result};
use chrono::NaiveDate;
use dashmap::DashSet;
use once_cell::sync::Lazy;
use regex::Regex;
use std::path::Path;
use tracing::{debug, info};
use walkdir::WalkDir;

// ---------------------------------------------------------------------------
// Regex patterns — 7 robust patterns with valid named capture groups
// ---------------------------------------------------------------------------

/// Pattern 1: `- [ ] **Category:** content` — unchecked task list, bold category
pub static RE_TASK_UNCHECKED: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"^\s*-\s*\[\s*\]\s+\*\*(?P<category>[A-Za-z][A-Za-z0-9_\-]*):\*\*\s+(?P<content>.+)$",
    )
    .expect("RE_TASK_UNCHECKED")
});

/// Pattern 2: `- [x] **Category:** content` — checked task list, bold category
pub static RE_TASK_CHECKED: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"^\s*-\s*\[[xX]\]\s+\*\*(?P<category>[A-Za-z][A-Za-z0-9_\-]*):\*\*\s+(?P<content>.+)$",
    )
    .expect("RE_TASK_CHECKED")
});

/// Pattern 3: `**Category:** content` — inline bold category
pub static RE_BOLD_CATEGORY: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\*\*(?P<category>[A-Za-z][A-Za-z0-9_\-]*):\*\*\s+(?P<content>.+)")
        .expect("RE_BOLD_CATEGORY")
});

/// Pattern 4: `- **Category:** content` — list item, bold category
pub static RE_LIST_BOLD: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"^\s*-\s+\*\*(?P<category>[A-Za-z][A-Za-z0-9_\-]*):\*\*\s+(?P<content>.+)$",
    )
    .expect("RE_LIST_BOLD")
});

/// Pattern 5: `- Category: content` — list item, plain category
pub static RE_LIST_PLAIN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*-\s+(?P<category>[A-Za-z][A-Za-z0-9_\-]*):\s+(?P<content>.+)$")
        .expect("RE_LIST_PLAIN")
});

/// Pattern 6: `Category: content` at start of line — standalone fact
pub static RE_STANDALONE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(?P<category>[A-Za-z][A-Za-z0-9_\-]*):\s+(?P<content>.+)$")
        .expect("RE_STANDALONE")
});

/// Pattern 7: `Category:: content` — double-colon separator
pub static RE_DOUBLECOLON: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*(?P<category>[A-Za-z][A-Za-z0-9_\-]*)::\s+(?P<content>.+)$")
        .expect("RE_DOUBLECOLON")
});

/// A pattern definition bundling a compiled regex with its confidence score.
pub struct PatternDef {
    pub re: &'static Lazy<Regex>,
    pub confidence: f64,
}

/// All 7 patterns in priority order (most specific first).
pub static ALL_PATTERNS: Lazy<Vec<PatternDef>> = Lazy::new(|| {
    vec![
        PatternDef {
            re: &RE_TASK_UNCHECKED,
            confidence: 0.85,
        },
        PatternDef {
            re: &RE_TASK_CHECKED,
            confidence: 0.90,
        },
        PatternDef {
            re: &RE_BOLD_CATEGORY,
            confidence: 0.95,
        },
        PatternDef {
            re: &RE_LIST_BOLD,
            confidence: 0.80,
        },
        PatternDef {
            re: &RE_LIST_PLAIN,
            confidence: 0.65,
        },
        PatternDef {
            re: &RE_STANDALONE,
            confidence: 0.70,
        },
        PatternDef {
            re: &RE_DOUBLECOLON,
            confidence: 0.50,
        },
    ]
});

// ---------------------------------------------------------------------------
// Extracted fact
// ---------------------------------------------------------------------------

/// A single extracted fact with metadata for dedup, sorting, and persistence.
#[derive(Debug, Clone)]
pub struct ExtractedFact {
    /// Canonical category (refined via nlp::refine_category)
    pub category: String,
    /// Fact content (trimmed, without leading/trailing whitespace)
    pub content: String,
    /// Source file path (relative or absolute)
    pub source_file: String,
    /// 1-based line number in source file
    pub line_number: usize,
    /// Extracted date (from filename or line), defaults to today
    pub date: Option<NaiveDate>,
    /// Confidence score (0.0 – 1.0), used for sorting
    pub confidence: f64,
}

// ---------------------------------------------------------------------------
// FactExtractor
// ---------------------------------------------------------------------------

/// Orchestrates extraction across all files in a directory.
pub struct FactExtractor {
    _config: MemoryExtractConfig,
    /// Global dedup set: `(category, content)` — ensures no duplicates.
    seen: DashSet<(String, String)>,
}

impl FactExtractor {
    pub fn new(config: MemoryExtractConfig) -> Self {
        Self {
            _config: config,
            seen: DashSet::new(),
        }
    }

    /// Extract all facts from every .md file under `root`.
    pub fn extract_all(&mut self, root: &Path) -> Result<Vec<ExtractedFact>> {
        info!("Extracting facts from {:?}", root);
        let mut results: Vec<ExtractedFact> = Vec::new();

        for entry in WalkDir::new(root)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.extension().map(|e| e == "md").unwrap_or(false) {
                debug!("Scanning {:?}", path);
                let facts = self
                    .extract_file(path)
                    .with_context(|| format!("Failed to extract from {:?}", path))?;
                results.extend(facts);
            }
        }

        // Deduplicate using total_cmp for f64 sorting
        results.sort_by(|a, b| b.confidence.total_cmp(&a.confidence));
        results.dedup_by(|a, b| a.category == b.category && a.content == b.content);

        let total = results.len();
        info!("Extracted {} unique facts", total);
        Ok(results)
    }

    /// Extract facts from a single file.
    pub fn extract_file(&self, path: &Path) -> Result<Vec<ExtractedFact>> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read {:?}", path))?;

        let source = path.to_string_lossy().into_owned();
        let date = date_from_path(path);

        let mut facts: Vec<ExtractedFact> = Vec::new();

        for (line_no, line) in raw.lines().enumerate() {
            // Skip very short or clearly non-fact lines for performance
            let trimmed = line.trim();
            if trimmed.len() < 6 {
                continue;
            }

            // Try each pattern, stop at first match per line
            for pat_def in ALL_PATTERNS.iter() {
                if let Some(caps) = pat_def.re.captures(line) {
                    let raw_cat = caps
                        .name("category")
                        .map(|m| m.as_str())
                        .unwrap_or("other");
                    let raw_content = caps.name("content").map(|m| m.as_str()).unwrap_or("");

                    let category = refine_category(raw_cat);
                    let content = raw_content.trim().to_string();

                    if content.is_empty() {
                        continue;
                    }

                    // Global dedup check
                    let key = (category.clone(), content.clone());
                    if self.seen.contains(&key) {
                        continue;
                    }
                    self.seen.insert(key);

                    facts.push(ExtractedFact {
                        category,
                        content,
                        source_file: source.clone(),
                        line_number: line_no + 1, // 1-based
                        date,
                        confidence: pat_def.confidence,
                    });

                    break; // one pattern per line
                }
            }
        }

        Ok(facts)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Try to extract a date from the filename.
///
/// Matches patterns like:
/// - `memory/2026-05-10.md`
/// - `2026-05-10-notes.md`
/// - `journal.2026-05-10.md`
fn date_from_path(path: &Path) -> Option<NaiveDate> {
    let stem = path.file_stem()?.to_str()?;

    // Try exact YYYY-MM-DD
    if let Ok(d) = NaiveDate::parse_from_str(stem, "%Y-%m-%d") {
        return Some(d);
    }

    // Try prefix YYYY-MM-DD (e.g. "2026-05-10-notes")
    if stem.len() >= 10 {
        if let Ok(d) = NaiveDate::parse_from_str(&stem[..10], "%Y-%m-%d") {
            return Some(d);
        }
    }

    // Try inline date via regex
    static RE_DATE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(\d{4}-\d{2}-\d{2})").expect("RE_DATE"));
    if let Some(cap) = RE_DATE.captures(stem) {
        if let Some(m) = cap.get(1) {
            if let Ok(d) = NaiveDate::parse_from_str(m.as_str(), "%Y-%m-%d") {
                return Some(d);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_task_unchecked() {
        let line = "- [ ] **Preference:** loves dark mode";
        let caps = RE_TASK_UNCHECKED.captures(line).unwrap();
        assert_eq!(caps.name("category").unwrap().as_str(), "Preference");
        assert_eq!(caps.name("content").unwrap().as_str(), "loves dark mode");
    }

    #[test]
    fn test_task_checked() {
        let line = "- [x] **Todo:** finish extractor";
        let caps = RE_TASK_CHECKED.captures(line).unwrap();
        assert_eq!(caps.name("category").unwrap().as_str(), "Todo");
        assert_eq!(caps.name("content").unwrap().as_str(), "finish extractor");
    }

    #[test]
    fn test_bold_category() {
        let line = "**Fact:** Earth is round";
        let caps = RE_BOLD_CATEGORY.captures(line).unwrap();
        assert_eq!(caps.name("category").unwrap().as_str(), "Fact");
        assert_eq!(caps.name("content").unwrap().as_str(), "Earth is round");
    }

    #[test]
    fn test_list_bold() {
        let line = "- **Decision:** use moka for caching";
        let caps = RE_LIST_BOLD.captures(line).unwrap();
        assert_eq!(caps.name("category").unwrap().as_str(), "Decision");
        assert_eq!(
            caps.name("content").unwrap().as_str(),
            "use moka for caching"
        );
    }

    #[test]
    fn test_list_plain() {
        let line = "- Pattern: check every hour";
        let caps = RE_LIST_PLAIN.captures(line).unwrap();
        assert_eq!(caps.name("category").unwrap().as_str(), "Pattern");
        assert_eq!(caps.name("content").unwrap().as_str(), "check every hour");
    }

    #[test]
    fn test_standalone() {
        let line = "Goal: ship v0.2 by Friday";
        let caps = RE_STANDALONE.captures(line).unwrap();
        assert_eq!(caps.name("category").unwrap().as_str(), "Goal");
        assert_eq!(
            caps.name("content").unwrap().as_str(),
            "ship v0.2 by Friday"
        );
    }

    #[test]
    fn test_doublecolon() {
        let line = "lesson:: always test before deploy";
        let caps = RE_DOUBLECOLON.captures(line).unwrap();
        assert_eq!(caps.name("category").unwrap().as_str(), "lesson");
        assert_eq!(
            caps.name("content").unwrap().as_str(),
            "always test before deploy"
        );
    }

    #[test]
    fn test_date_from_filename() {
        let p = PathBuf::from("memory/2026-05-10.md");
        let d = date_from_path(&p);
        assert!(d.is_some());
        assert_eq!(d.unwrap().to_string(), "2026-05-10");
    }
}
