//! The two pluggable roles of the loop: who *proposes* a change and who
//! *judges* it. Keeping both behind traits is deliberate — it is exactly the
//! STOP decomposition (Zelikman et al., 2023), where the "improver" is just a
//! function the system can swap out (heuristic, LLM, or another evolved
//! improver), and it keeps the core loop unit-testable with no LLM or `cargo`
//! in the loop.

use crate::error::Result;
use crate::rng::SplitMix64;
use crate::variant::{Fitness, Patch};
use std::path::{Path, PathBuf};

/// A minimal text-completion model. An LLM adapter implements this, but so does
/// a deterministic stub used in tests. The core never depends on a concrete LLM
/// crate — the improver is "just a function" (STOP).
pub trait CodeModel {
    fn complete(&self, prompt: &str) -> Result<String>;
}

/// Everything a proposer needs to reason about the next change.
pub struct ImprovementContext<'a> {
    /// Root of the (live, read-only) workspace the proposer may inspect.
    pub workspace_root: &'a Path,
    /// High-level objective the improvement should serve.
    pub goal: &'a str,
    /// Fitness of the parent the change will branch from.
    pub parent_fitness: Option<&'a Fitness>,
    /// Rationales of recently rejected attempts, so the proposer can avoid
    /// repeating dead ends (cheap "verbal reinforcement", cf. Reflexion).
    pub recent_rejections: &'a [String],
}

impl ImprovementContext<'_> {
    /// Read a source file relative to the workspace root.
    pub fn read(&self, rel: &str) -> std::io::Result<String> {
        std::fs::read_to_string(self.resolve(rel))
    }

    pub fn resolve(&self, rel: &str) -> PathBuf {
        self.workspace_root.join(rel)
    }
}

/// A proposed self-modification with the reasoning behind it.
#[derive(Debug, Clone)]
pub struct Proposal {
    pub patch: Patch,
    pub rationale: String,
}

/// Generates the next candidate edit. Returning `None` means "no useful change
/// this step" — a legitimate, common outcome that the loop must tolerate.
pub trait Proposer {
    fn propose(
        &self,
        ctx: &ImprovementContext<'_>,
        rng: &mut SplitMix64,
    ) -> Result<Option<Proposal>>;
}

/// Measures a candidate workspace empirically. The contract: never trust a
/// proposal's self-assessment — build it and run the tests. This is the
/// non-negotiable validation gate of the Darwin Gödel Machine.
pub trait Evaluator {
    /// Evaluate the workspace rooted at `workspace` (an isolated copy with the
    /// candidate patch already applied). Must not touch the live tree.
    fn evaluate(&self, workspace: &Path) -> Result<Fitness>;
}
