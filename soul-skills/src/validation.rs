//! Validated skill retention — the empirical gate for induced skills.
//!
//! Hermes-Agent refines skills "during use" with no validation gate, so it can
//! regress silently. SoulSystem's moat (see `docs/RESEARCH_FRONTIER_2026.md` §4,
//! SEVerA / Voyager) is that a synthesised skill is **retained only if it passes
//! verification** — the same discipline `soul-rsi` applies to code changes.
//!
//! This module supplies that gate for the [`Skill`] library. [`SkillFitness`]
//! mirrors `soul_rsi::Fitness`: a hard validity gate dominates the scalar quality
//! score, so a malformed skill can never out-rank a well-formed one regardless of
//! how high it scores. [`ValidatedSkillLibrary`] wires
//! [`SkillInducer`](crate::SkillInducer) → validation → retention so the library
//! grows only with skills that clear the bar.

use std::collections::HashSet;

use crate::induction::{Episode, Induction, SkillInducer};
use crate::Skill;

/// Structural/quality measurement of a candidate skill.
///
/// Like `soul_rsi::Fitness`, the ordering is lexicographic with a hard gate
/// first: `valid` dominates `score`. A skill that fails a hard check
/// (`valid == false`) can never be judged better than one that passes.
#[derive(Debug, Clone, PartialEq)]
pub struct SkillFitness {
    /// Whether the candidate cleared every hard check.
    pub valid: bool,
    /// Quality scalar (higher is better); only compared once `valid` is equal.
    pub score: f64,
    /// Human-readable reasons the candidate failed or warnings about it.
    pub issues: Vec<String>,
}

impl SkillFitness {
    /// Strictly better than `other` under the gate ordering (valid first, then
    /// score). NaN scores are treated as the floor.
    pub fn is_better_than(&self, other: &SkillFitness) -> bool {
        match (self.valid, other.valid) {
            (true, false) => true,
            (false, true) => false,
            _ => self.score.partial_cmp(&other.score) == Some(std::cmp::Ordering::Greater),
        }
    }
}

/// Judges whether a synthesised skill should be retained in a library.
pub trait SkillValidator {
    /// Measure `candidate` in the context of the current `library`.
    fn validate(&self, candidate: &Skill, library: &[Skill]) -> SkillFitness;
}

/// The default, deterministic validator — no LLM. Enforces the quality bar that
/// keeps the library coherent and useful.
#[derive(Debug, Clone)]
pub struct StructuralValidator {
    /// A reusable procedure needs at least this many steps.
    pub min_steps: usize,
    /// A discoverable skill needs at least this many triggers.
    pub min_triggers: usize,
    /// Longest acceptable skill name.
    pub max_name_len: usize,
    /// If set, every `tools_required` entry must be in this allow-list (keeps
    /// the agent from inducing skills that reference tools it doesn't have).
    pub allowed_tools: Option<HashSet<String>>,
}

impl Default for StructuralValidator {
    fn default() -> Self {
        Self {
            min_steps: 2,
            min_triggers: 1,
            max_name_len: 64,
            allowed_tools: None,
        }
    }
}

impl SkillValidator for StructuralValidator {
    fn validate(&self, candidate: &Skill, library: &[Skill]) -> SkillFitness {
        let mut issues = Vec::new();

        // ── Hard checks (any failure ⇒ invalid). ──────────────────────────
        if candidate.name.trim().is_empty() {
            issues.push("name is empty".into());
        }
        if !crate::is_safe_skill_name(&candidate.name) {
            issues.push(format!(
                "name {:?} is not a safe filename (must be a single path segment: no '.', '..', '/', '\\', or NUL)",
                candidate.name
            ));
        }
        if candidate.name.len() > self.max_name_len {
            issues.push(format!(
                "name exceeds {} chars ({})",
                self.max_name_len,
                candidate.name.len()
            ));
        }
        if candidate.description.trim().is_empty() {
            issues.push("description is empty".into());
        }
        if candidate.steps.len() < self.min_steps {
            issues.push(format!(
                "needs >= {} steps, has {}",
                self.min_steps,
                candidate.steps.len()
            ));
        }
        if candidate.triggers.len() < self.min_triggers {
            issues.push(format!(
                "needs >= {} triggers, has {}",
                self.min_triggers,
                candidate.triggers.len()
            ));
        }
        if candidate.triggers.iter().any(|t| t.trim().is_empty()) {
            issues.push("has an empty trigger".into());
        }
        if let Some(allowed) = &self.allowed_tools {
            for tool in &candidate.tools_required {
                if !allowed.contains(tool) {
                    issues.push(format!("unknown tool '{tool}'"));
                }
            }
        }
        // A *new* name must not collide with a different existing skill — that
        // would shadow it. (Reinforcement reuses the existing name, so the
        // candidate name equals the matched skill and this does not fire.)
        if library
            .iter()
            .any(|s| s.name == candidate.name && s != candidate)
        {
            issues.push(format!(
                "name '{}' collides with a different existing skill",
                candidate.name
            ));
        }

        let valid = issues.is_empty();

        // ── Quality score (only meaningful once valid). ───────────────────
        // Reward richer, more reusable procedures; mild diminishing returns.
        let step_score = (candidate.steps.len() as f64).min(8.0) / 8.0;
        let trigger_score = (candidate.triggers.len() as f64).min(6.0) / 6.0;
        let example_bonus = if candidate.examples.is_empty() {
            0.0
        } else {
            0.15
        };
        let tool_bonus = if candidate.tools_required.is_empty() {
            0.0
        } else {
            0.1
        };
        let score = (0.5 * step_score + 0.25 * trigger_score + example_bonus + tool_bonus).min(1.0);

        SkillFitness {
            valid,
            score,
            issues,
        }
    }
}

/// The outcome of offering an [`Episode`] to a [`ValidatedSkillLibrary`].
#[derive(Debug, Clone, PartialEq)]
pub enum Retention {
    /// A new skill cleared the gate and was added to the library.
    Added(Skill),
    /// An existing skill was reinforced and the update cleared the gate.
    Reinforced(Skill),
    /// A candidate was synthesised but failed validation; the library is
    /// unchanged. Carries the candidate name and the gate's issues.
    Rejected {
        candidate: String,
        fitness: SkillFitness,
    },
    /// Induction produced no candidate (failed/trivial episode).
    Skipped(&'static str),
}

/// A skill library that only admits induced skills which pass a validation gate.
///
/// This is the unification the roadmap calls for: [`SkillInducer`] proposes,
/// the [`SkillValidator`] judges, and only validated skills are retained — the
/// SEVerA "verify-before-retain" discipline applied to the skill library.
#[derive(Debug, Clone)]
pub struct ValidatedSkillLibrary<V: SkillValidator = StructuralValidator> {
    inducer: SkillInducer,
    validator: V,
    skills: Vec<Skill>,
}

impl ValidatedSkillLibrary<StructuralValidator> {
    /// A library seeded with `initial` skills, the default inducer, and the
    /// default structural validator.
    pub fn new(initial: Vec<Skill>) -> Self {
        Self {
            inducer: SkillInducer::default(),
            validator: StructuralValidator::default(),
            skills: initial,
        }
    }
}

impl<V: SkillValidator> ValidatedSkillLibrary<V> {
    /// Build a library from explicit parts.
    pub fn with_parts(inducer: SkillInducer, validator: V, initial: Vec<Skill>) -> Self {
        Self {
            inducer,
            validator,
            skills: initial,
        }
    }

    /// The current retained skills.
    pub fn skills(&self) -> &[Skill] {
        &self.skills
    }

    /// Number of retained skills.
    pub fn len(&self) -> usize {
        self.skills.len()
    }

    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }

    /// Offer a completed task to the library: induce a skill, validate it, and
    /// retain it only if it clears the gate. Returns what happened.
    pub fn offer(&mut self, episode: &Episode) -> Retention {
        match self.inducer.induce_or_merge(episode, &self.skills) {
            Induction::New(candidate) => {
                let fitness = self.validator.validate(&candidate, &self.skills);
                if fitness.valid {
                    self.skills.push(candidate.clone());
                    Retention::Added(candidate)
                } else {
                    Retention::Rejected {
                        candidate: candidate.name,
                        fitness,
                    }
                }
            }
            Induction::Reinforced(updated) => {
                // Validate the reinforced skill against the library *excluding*
                // the skill it replaces, so its own name does not read as a
                // collision.
                let others: Vec<Skill> = self
                    .skills
                    .iter()
                    .filter(|s| s.name != updated.name)
                    .cloned()
                    .collect();
                let fitness = self.validator.validate(&updated, &others);
                if fitness.valid {
                    if let Some(slot) = self.skills.iter_mut().find(|s| s.name == updated.name) {
                        *slot = updated.clone();
                    } else {
                        self.skills.push(updated.clone());
                    }
                    Retention::Reinforced(updated)
                } else {
                    Retention::Rejected {
                        candidate: updated.name,
                        fitness,
                    }
                }
            }
            Induction::Skipped(reason) => Retention::Skipped(reason),
        }
    }

    /// Offer an episode and, when a skill is added or reinforced, persist it to
    /// disk via the loader so the evolved library survives a restart — the
    /// "persistent skills" property Hermes-Agent has, but here gated on
    /// validation. Rejected/skipped outcomes write nothing.
    pub async fn offer_and_persist(
        &mut self,
        episode: &Episode,
        loader: &crate::SkillLoader,
    ) -> crate::Result<Retention> {
        let outcome = self.offer(episode);
        if let Retention::Added(skill) | Retention::Reinforced(skill) = &outcome {
            loader.save_skill(skill).await?;
        }
        Ok(outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::induction::EpisodeAction;

    fn good_episode() -> Episode {
        Episode::new(
            "Review the auth module for security bugs",
            vec![
                EpisodeAction::new("read_file", "Read the auth module source"),
                EpisodeAction::new("grep", "Search for credential handling"),
            ],
            true,
        )
    }

    #[test]
    fn fitness_validity_gate_dominates_score() {
        let invalid_high = SkillFitness {
            valid: false,
            score: 1.0,
            issues: vec!["x".into()],
        };
        let valid_low = SkillFitness {
            valid: true,
            score: 0.1,
            issues: vec![],
        };
        assert!(valid_low.is_better_than(&invalid_high));
        assert!(!invalid_high.is_better_than(&valid_low));
    }

    #[test]
    fn structural_validator_accepts_well_formed_skill() {
        let v = StructuralValidator::default();
        let candidate = Skill {
            triggers: vec!["auth".into()],
            steps: vec!["a".into(), "b".into()],
            ..Skill::new("review-auth", "Review auth")
        };
        let f = v.validate(&candidate, &[]);
        assert!(f.valid, "issues: {:?}", f.issues);
        assert!(f.score > 0.0);
    }

    #[test]
    fn structural_validator_rejects_thin_skill() {
        let v = StructuralValidator::default();
        let candidate = Skill {
            triggers: vec![],
            steps: vec!["only one".into()],
            ..Skill::new("thin", "Thin skill")
        };
        let f = v.validate(&candidate, &[]);
        assert!(!f.valid);
        assert!(f.issues.iter().any(|i| i.contains("steps")));
        assert!(f.issues.iter().any(|i| i.contains("triggers")));
    }

    #[test]
    fn structural_validator_enforces_tool_allowlist() {
        let v = StructuralValidator {
            allowed_tools: Some(["read_file".to_string()].into_iter().collect()),
            ..StructuralValidator::default()
        };
        let candidate = Skill {
            triggers: vec!["x".into()],
            steps: vec!["a".into(), "b".into()],
            tools_required: vec!["read_file".into(), "rm_rf".into()],
            ..Skill::new("danger", "Uses an unknown tool")
        };
        let f = v.validate(&candidate, &[]);
        assert!(!f.valid);
        assert!(f.issues.iter().any(|i| i.contains("rm_rf")));
    }

    #[test]
    fn structural_validator_rejects_path_traversal_name() {
        let v = StructuralValidator::default();
        let candidate = Skill {
            triggers: vec!["x".into()],
            steps: vec!["a".into(), "b".into()],
            ..Skill::new("../../evil", "Path traversal via name")
        };
        let f = v.validate(&candidate, &[]);
        assert!(!f.valid);
        assert!(f.issues.iter().any(|i| i.contains("safe filename")));
    }

    #[test]
    fn structural_validator_flags_name_collision() {
        let v = StructuralValidator::default();
        let existing = Skill {
            triggers: vec!["x".into()],
            steps: vec!["a".into(), "b".into()],
            ..Skill::new("dup", "first")
        };
        let candidate = Skill {
            triggers: vec!["y".into()],
            steps: vec!["c".into(), "d".into()],
            ..Skill::new("dup", "second, different body")
        };
        let f = v.validate(&candidate, std::slice::from_ref(&existing));
        assert!(!f.valid);
        assert!(f.issues.iter().any(|i| i.contains("collides")));
    }

    #[test]
    fn library_adds_validated_new_skill() {
        let mut lib = ValidatedSkillLibrary::new(vec![]);
        match lib.offer(&good_episode()) {
            Retention::Added(s) => assert_eq!(s.name, "review-auth-module-security"),
            other => panic!("expected Added, got {other:?}"),
        }
        assert_eq!(lib.len(), 1);
    }

    #[test]
    fn library_reinforces_existing_skill() {
        let existing = Skill {
            triggers: vec!["auth".into(), "security".into()],
            steps: vec!["look".into(), "report".into()],
            tools_required: vec!["read_file".into()],
            priority: 6,
            ..Skill::new("security-audit", "Audit for security issues")
        };
        let mut lib = ValidatedSkillLibrary::new(vec![existing]);

        match lib.offer(&good_episode()) {
            Retention::Reinforced(s) => {
                assert_eq!(s.name, "security-audit");
                assert_eq!(s.priority, 7); // bumped
                assert!(s.tools_required.contains(&"grep".to_string())); // unioned
            }
            other => panic!("expected Reinforced, got {other:?}"),
        }
        // Library size unchanged — reinforcement updates in place.
        assert_eq!(lib.len(), 1);
    }

    #[test]
    fn library_skips_failed_episode() {
        let mut lib = ValidatedSkillLibrary::new(vec![]);
        let mut failed = good_episode();
        failed.success = false;
        assert!(matches!(lib.offer(&failed), Retention::Skipped(_)));
        assert!(lib.is_empty());
    }

    #[test]
    fn library_rejects_when_validator_fails() {
        // A validator that rejects everything (e.g. an empty tool allow-list).
        let inducer = SkillInducer::default();
        let validator = StructuralValidator {
            allowed_tools: Some(HashSet::new()),
            ..StructuralValidator::default()
        };
        let mut lib = ValidatedSkillLibrary::with_parts(inducer, validator, vec![]);

        match lib.offer(&good_episode()) {
            Retention::Rejected { candidate, fitness } => {
                assert_eq!(candidate, "review-auth-module-security");
                assert!(!fitness.valid);
            }
            other => panic!("expected Rejected, got {other:?}"),
        }
        assert!(lib.is_empty());
    }

    #[tokio::test]
    async fn offer_and_persist_writes_validated_skill_to_disk() {
        use crate::SkillLoader;

        let dir = tempfile::tempdir().unwrap();
        let loader = SkillLoader::new(dir.path());
        let mut lib = ValidatedSkillLibrary::new(vec![]);

        let outcome = lib
            .offer_and_persist(&good_episode(), &loader)
            .await
            .unwrap();
        assert!(matches!(outcome, Retention::Added(_)));

        // The induced skill was written and round-trips back through the loader.
        let path = dir.path().join("review-auth-module-security.md");
        assert!(path.exists(), "skill markdown should be persisted");
        let reloaded = loader.load_skill_file(&path).await.unwrap();
        assert_eq!(reloaded.name, "review-auth-module-security");
        assert!(reloaded.triggers.contains(&"auth".to_string()));
    }

    #[tokio::test]
    async fn offer_and_persist_writes_nothing_on_skip() {
        use crate::SkillLoader;

        let dir = tempfile::tempdir().unwrap();
        let loader = SkillLoader::new(dir.path());
        let mut lib = ValidatedSkillLibrary::new(vec![]);

        let mut failed = good_episode();
        failed.success = false;
        let outcome = lib.offer_and_persist(&failed, &loader).await.unwrap();
        assert!(matches!(outcome, Retention::Skipped(_)));

        let mut entries = tokio::fs::read_dir(dir.path()).await.unwrap();
        assert!(
            entries.next_entry().await.unwrap().is_none(),
            "no file written"
        );
    }
}
