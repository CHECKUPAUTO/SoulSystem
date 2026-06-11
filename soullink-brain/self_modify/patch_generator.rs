use crate::self_modify::mutation_strategies::MutationStrategy;
use uuid::Uuid;

/// Origin of a patch candidate.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PatchOrigin {
    /// Generated from static AST analysis.
    StaticAnalysis,
    /// Generated from formal verification feedback.
    FormalFeedback,
    /// Generated from evolutionary search.
    Evolution,
}

/// A candidate patch produced by the generator.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PatchCandidate {
    pub id: Uuid,
    pub diff: String,
    pub strategy: MutationStrategy,
    pub origin: PatchOrigin,
}

/// Generator trait for patch candidates.
pub trait PatchGenerator {
    fn generate(&self, report: &crate::self_modify::analyzer::CodeReport) -> anyhow::Result<Vec<PatchCandidate>>;
}

/// Default patch generator.
pub struct DefaultPatchGenerator {
    workspace: std::path::PathBuf,
}

impl DefaultPatchGenerator {
    /// Create a new default patch generator.
    pub fn new(workspace: impl Into<std::path::PathBuf>) -> Self {
        Self { workspace: workspace.into() }
    }
}

impl PatchGenerator for DefaultPatchGenerator {
    fn generate(&self, _report: &crate::self_modify::analyzer::CodeReport) -> anyhow::Result<Vec<PatchCandidate>> {
        // NOTE: implement real AST-based generation
        Ok(vec![
            PatchCandidate {
                id: Uuid::new_v4(),
                diff: "// placeholder diff".into(),
                strategy: MutationStrategy::CloneToRef,
                origin: PatchOrigin::StaticAnalysis,
            }
        ])
    }
}
