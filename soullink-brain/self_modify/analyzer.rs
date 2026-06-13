use std::path::Path;
use anyhow::Result;

/// Report produced by static code analysis.
#[derive(Debug, Clone, Default)]
pub struct CodeReport {
    pub opportunities: Vec<String>,
}

impl CodeReport {
    /// Return true if the report contains improvement opportunities.
    pub fn has_improvement_opportunities(&self) -> bool {
        !self.opportunities.is_empty()
    }
}

/// Analyzer trait.
pub trait Analyzer {
    fn analyze(&self, workspace: &Path) -> Result<CodeReport>;
}

/// Syn-based analyzer.
pub struct SynAnalyzer;

impl SynAnalyzer {
    /// Create a new syn-based analyzer.
    pub fn new() -> Self { Self }
}

impl Analyzer for SynAnalyzer {
    fn analyze(&self, _workspace: &Path) -> Result<CodeReport> {
        // NOTE: implement real syn-based analysis
        Ok(CodeReport {
            opportunities: vec!["placeholder opportunity".into()],
        })
    }
}
