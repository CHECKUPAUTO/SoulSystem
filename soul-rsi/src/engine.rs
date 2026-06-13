//! The recursive self-improvement loop.
//!
//! ```text
//!   ┌─────────────── archive (open-ended population of accepted variants)
//!   │                         │ select_parent (quality × novelty)
//!   │                         ▼
//!   │                  Proposer.propose ──► candidate patch
//!   │                         │
//!   │                 snapshot live tree, apply patch (isolated copy)
//!   │                         │
//!   │                  Evaluator.evaluate ──► Fitness (build + tests)
//!   │                         │
//!   │      is_better_than(parent) && passes gate ?
//!   │            yes ─► insert into archive        no ─► discard, remember
//!   └─────────────────────────┘
//! ```
//!
//! This is the empirical core shared by STOP (the proposer is a swappable
//! "improver") and the Darwin Gödel Machine (keep every validated stepping
//! stone, branch from any of them, trust only measured fitness).

use crate::archive::Archive;
use crate::error::Result;
use crate::rng::SplitMix64;
use crate::snapshot::WorkspaceSnapshot;
use crate::traits::{Evaluator, ImprovementContext, Proposer};
use crate::variant::{Fitness, Status, Variant};
use std::path::PathBuf;

/// Tuning for the loop.
#[derive(Debug, Clone)]
pub struct RsiConfig {
    /// Live workspace root that gets snapshotted for each evaluation.
    pub workspace_root: PathBuf,
    /// High-level objective handed to the proposer.
    pub goal: String,
    /// If true, a variant is only accepted when its whole test suite is green
    /// (compiles and zero failures). Strongly recommended for unattended runs.
    pub accept_requires_all_green: bool,
    /// How many recent rejection rationales to feed back to the proposer.
    pub rejection_memory: usize,
}

impl RsiConfig {
    pub fn new(workspace_root: impl Into<PathBuf>, goal: impl Into<String>) -> Self {
        Self {
            workspace_root: workspace_root.into(),
            goal: goal.into(),
            accept_requires_all_green: true,
            rejection_memory: 8,
        }
    }
}

/// What happened in one step — the audited unit of progress.
#[derive(Debug, Clone)]
pub enum StepOutcome {
    /// The proposer declined to suggest a change this step.
    NoProposal,
    /// A candidate was evaluated. `accepted` is true iff it entered the archive.
    Evaluated {
        variant_id: String,
        parent_id: Option<String>,
        accepted: bool,
        fitness: Fitness,
    },
}

impl StepOutcome {
    pub fn accepted(&self) -> bool {
        matches!(self, StepOutcome::Evaluated { accepted: true, .. })
    }
}

/// The self-improvement engine. Generic over the proposer and evaluator so the
/// exact same loop drives unit tests (deterministic stubs) and production
/// (LLM + `cargo`).
pub struct RsiEngine<P: Proposer, E: Evaluator> {
    archive: Archive,
    proposer: P,
    evaluator: E,
    config: RsiConfig,
    rng: SplitMix64,
    recent_rejections: Vec<String>,
    history: Vec<StepOutcome>,
}

impl<P: Proposer, E: Evaluator> RsiEngine<P, E> {
    pub fn new(archive: Archive, proposer: P, evaluator: E, config: RsiConfig, seed: u64) -> Self {
        Self {
            archive,
            proposer,
            evaluator,
            config,
            rng: SplitMix64::new(seed),
            recent_rejections: Vec::new(),
            history: Vec::new(),
        }
    }

    pub fn archive(&self) -> &Archive {
        &self.archive
    }

    pub fn history(&self) -> &[StepOutcome] {
        &self.history
    }

    pub fn best(&self) -> Option<&Variant> {
        self.archive.best()
    }

    /// Run one propose → evaluate → select step.
    pub fn step(&mut self) -> Result<StepOutcome> {
        // 1. Pick a parent to branch from (open-ended selection).
        let parent = match self.archive.select_parent(&mut self.rng) {
            Some(v) => v.clone(),
            None => {
                let o = StepOutcome::NoProposal;
                self.history.push(o.clone());
                return Ok(o);
            }
        };

        // 2. Ask the proposer for a change. Disjoint field borrows keep this safe.
        let rejections = self.recent_rejections.clone();
        let proposal = {
            let ctx = ImprovementContext {
                workspace_root: &self.config.workspace_root,
                goal: &self.config.goal,
                parent_fitness: parent.fitness.as_ref(),
                recent_rejections: &rejections,
            };
            self.proposer.propose(&ctx, &mut self.rng)?
        };
        let proposal = match proposal {
            Some(p) if !p.patch.is_noop() => p,
            _ => {
                let o = StepOutcome::NoProposal;
                self.history.push(o.clone());
                return Ok(o);
            }
        };

        // 3. Evaluate the candidate in an isolated snapshot.
        let fitness = match self.evaluate_candidate(&proposal.patch) {
            Ok(f) => f,
            Err(e) => Fitness::broken(format!("could not evaluate: {e}")),
        };

        // 4. Accept only if it beats the parent under the gated ordering.
        let parent_fit = parent
            .fitness
            .clone()
            .unwrap_or_else(|| Fitness::broken("parent had no fitness"));
        let gate_ok = !self.config.accept_requires_all_green || fitness.all_green();
        let accepted = gate_ok && fitness.is_better_than(&parent_fit);

        let mut child = Variant::child(&parent, proposal.patch, proposal.rationale);
        child.fitness = Some(fitness.clone());
        child.status = if accepted {
            Status::Accepted
        } else {
            Status::Rejected
        };

        if accepted {
            tracing::info!(
                variant = %child.id,
                generation = child.generation,
                score = fitness.score,
                "rsi: accepted improvement"
            );
            self.archive.insert(child.clone());
        } else {
            self.remember_rejection(&child.rationale);
        }

        let outcome = StepOutcome::Evaluated {
            variant_id: child.id,
            parent_id: child.parent,
            accepted,
            fitness,
        };
        self.history.push(outcome.clone());
        Ok(outcome)
    }

    /// Run up to `max_steps`, returning every step's outcome.
    pub fn run(&mut self, max_steps: usize) -> Result<Vec<StepOutcome>> {
        let mut out = Vec::with_capacity(max_steps);
        for _ in 0..max_steps {
            out.push(self.step()?);
        }
        Ok(out)
    }

    fn evaluate_candidate(&self, patch: &crate::variant::Patch) -> Result<Fitness> {
        let snap = WorkspaceSnapshot::create(&self.config.workspace_root)?;
        if let Err(e) = snap.apply(patch) {
            // A patch that does not apply (pattern missing) is a failed candidate,
            // not a crash of the loop.
            return Ok(Fitness::broken(format!("patch did not apply: {e}")));
        }
        self.evaluator.evaluate(snap.root())
    }

    fn remember_rejection(&mut self, rationale: &str) {
        self.recent_rejections.push(rationale.to_string());
        let cap = self.config.rejection_memory;
        if self.recent_rejections.len() > cap {
            let overflow = self.recent_rejections.len() - cap;
            self.recent_rejections.drain(0..overflow);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::{Proposal, Proposer};
    use crate::variant::Patch;
    use std::path::Path;
    use tempfile::TempDir;

    /// A toy workspace: a file holding a single integer `level = N`. "Better"
    /// means a higher N. The proposer increments it; the evaluator scores N.
    fn toy_workspace() -> TempDir {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/level.txt"), "level = 0").unwrap();
        dir
    }

    fn read_level(root: &Path) -> i64 {
        std::fs::read_to_string(root.join("src/level.txt"))
            .unwrap()
            .trim()
            .strip_prefix("level = ")
            .and_then(|s| s.parse().ok())
            .unwrap_or(-1)
    }

    /// Proposer that increments the level by one each time.
    struct Incrementer;
    impl Proposer for Incrementer {
        fn propose(
            &self,
            ctx: &ImprovementContext<'_>,
            _rng: &mut SplitMix64,
        ) -> Result<Option<Proposal>> {
            let cur = read_level(ctx.workspace_root);
            let next = cur + 1;
            Ok(Some(Proposal {
                patch: Patch::new(
                    "src/level.txt",
                    format!("level = {cur}"),
                    format!("level = {next}"),
                ),
                rationale: format!("raise level to {next}"),
            }))
        }
    }

    /// Evaluator that rewards a higher level; always compiles & green.
    fn level_evaluator() -> crate::evaluator::ClosureEvaluator<impl Fn(&Path) -> Fitness> {
        crate::evaluator::ClosureEvaluator::new(|root: &Path| Fitness {
            compiles: true,
            tests_passed: 1,
            tests_failed: 0,
            score: read_level(root) as f64,
            notes: String::new(),
        })
    }

    fn engine(
        ws: &Path,
    ) -> RsiEngine<Incrementer, crate::evaluator::ClosureEvaluator<impl Fn(&Path) -> Fitness>> {
        let baseline = Fitness {
            compiles: true,
            tests_passed: 1,
            tests_failed: 0,
            score: 0.0,
            notes: "baseline".to_string(),
        };
        let archive = Archive::with_root(baseline);
        let config = RsiConfig::new(ws, "raise the level");
        RsiEngine::new(archive, Incrementer, level_evaluator(), config, 1)
    }

    #[test]
    fn loop_accumulates_only_real_improvements() {
        let ws = toy_workspace();
        let mut eng = engine(ws.path());
        eng.run(5).unwrap();
        // Root + at least one accepted improvement; best score must exceed baseline.
        assert!(eng.archive().len() >= 2, "no improvement was archived");
        let best = eng.best().unwrap();
        assert!(best.fitness.as_ref().unwrap().score >= 1.0);
        // The live workspace is never mutated by the loop.
        assert_eq!(read_level(ws.path()), 0);
    }

    /// Proposer that always proposes a build-breaking change.
    struct Saboteur;
    impl Proposer for Saboteur {
        fn propose(
            &self,
            _ctx: &ImprovementContext<'_>,
            _rng: &mut SplitMix64,
        ) -> Result<Option<Proposal>> {
            Ok(Some(Proposal {
                patch: Patch::new("src/level.txt", "level = 0", "level = 0 BROKEN"),
                rationale: "sabotage".to_string(),
            }))
        }
    }

    #[test]
    fn regressions_are_rejected() {
        let ws = toy_workspace();
        let baseline = Fitness {
            compiles: true,
            tests_passed: 1,
            tests_failed: 0,
            score: 0.0,
            notes: String::new(),
        };
        // Evaluator marks the broken marker as a compile failure.
        let evaluator = crate::evaluator::ClosureEvaluator::new(|root: &Path| {
            let txt = std::fs::read_to_string(root.join("src/level.txt")).unwrap_or_default();
            if txt.contains("BROKEN") {
                Fitness::broken("syntax error")
            } else {
                Fitness {
                    compiles: true,
                    tests_passed: 1,
                    tests_failed: 0,
                    score: 0.0,
                    notes: String::new(),
                }
            }
        });
        let mut eng = RsiEngine::new(
            Archive::with_root(baseline),
            Saboteur,
            evaluator,
            RsiConfig::new(ws.path(), "break things"),
            1,
        );
        eng.run(4).unwrap();
        // Nothing accepted: archive still only holds the root.
        assert_eq!(eng.archive().len(), 1);
        assert!(eng.history().iter().all(|o| !o.accepted()));
    }

    #[test]
    fn run_is_seed_deterministic() {
        let ws1 = toy_workspace();
        let ws2 = toy_workspace();
        let mut a = engine(ws1.path());
        let mut b = engine(ws2.path());
        a.run(5).unwrap();
        b.run(5).unwrap();
        assert_eq!(a.archive().len(), b.archive().len());
        assert_eq!(
            a.best().unwrap().fitness.as_ref().unwrap().score,
            b.best().unwrap().fitness.as_ref().unwrap().score
        );
    }
}
