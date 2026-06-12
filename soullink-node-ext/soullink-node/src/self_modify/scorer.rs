use crate::self_modify::patch_validator::ValidationReport;

/// Multi-objective score for a patch.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct PatchScore {
    pub total: f32,
    pub breakdown: ScoreBreakdown,
}

/// Score breakdown.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ScoreBreakdown {
    pub correctness: f32,
    pub performance: f32,
    pub readability: f32,
    pub safety: f32,
}

/// Trait for patch scorers.
pub trait Scorer {
    fn score(&self, validation: &ValidationReport) -> PatchScore;
}

/// Default scorer using a weighted formula.
pub struct DefaultScorer;

impl DefaultScorer {
    /// Create a new default scorer.
    pub fn new() -> Self { Self }
}

impl Scorer for DefaultScorer {
    fn score(&self, validation: &ValidationReport) -> PatchScore {
        let correctness = if validation.build_ok { 0.3 } else { 0.0 };
        let tests = if validation.tests_ok { 0.3 } else { 0.0 };
        let lint = if validation.lint_ok { 0.2 } else { 0.0 };
        let warnings = if validation.warnings.is_empty() { 0.2 } else { 0.1 };
        let total = correctness + tests + lint + warnings;
        PatchScore {
            total,
            breakdown: ScoreBreakdown {
                correctness,
                performance: 0.0,
                readability: 0.0,
                safety: lint + warnings,
            },
        }
    }
}
