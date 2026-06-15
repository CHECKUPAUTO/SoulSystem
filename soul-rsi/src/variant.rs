//! Variants and fitness — the unit of recursive self-improvement.
//!
//! A [`Variant`] is one candidate self-modification: a patch to a single source
//! file, plus the lineage and empirical [`Fitness`] that justify keeping it.
//! This mirrors the "stepping stone" object of the Darwin Gödel Machine
//! (Zhang et al., 2025) and the improver output of STOP (Zelikman et al., 2023).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A concrete, reversible edit to one source file.
///
/// The edit is expressed as an exact `find` → `replace` substitution so it can
/// be applied (and audited) deterministically via `soul_automodify`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Patch {
    /// Path of the file to edit, relative to the workspace root.
    pub target: String,
    /// Exact text fragment to replace. Must occur exactly once.
    pub find: String,
    /// Replacement text.
    pub replace: String,
}

impl Patch {
    pub fn new(
        target: impl Into<String>,
        find: impl Into<String>,
        replace: impl Into<String>,
    ) -> Self {
        Self {
            target: target.into(),
            find: find.into(),
            replace: replace.into(),
        }
    }

    /// A patch is a no-op if it does not change anything.
    pub fn is_noop(&self) -> bool {
        self.find == self.replace
    }
}

/// Empirically measured quality of a variant.
///
/// The ordering is lexicographic and matches what a careful engineer would
/// accept: a change must (1) still compile, (2) not regress the test suite, and
/// only then (3) is its scalar `score` compared. Encoding the gate this way is
/// what makes the loop *safe* — unlike a pure scalar objective, a variant that
/// breaks the build can never dominate one that builds.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Fitness {
    /// Whether the variant's workspace compiled.
    pub compiles: bool,
    /// Number of tests that passed.
    pub tests_passed: u32,
    /// Number of tests that failed.
    pub tests_failed: u32,
    /// Domain-specific scalar reward (higher is better). Only compared once the
    /// compile/test gates are equal.
    pub score: f64,
    /// Free-form notes (compiler errors, evaluator remarks).
    pub notes: String,
}

impl Fitness {
    /// The fitness of a variant that failed to build — the absolute floor.
    pub fn broken(notes: impl Into<String>) -> Self {
        Self {
            compiles: false,
            tests_passed: 0,
            tests_failed: 0,
            score: f64::NEG_INFINITY,
            notes: notes.into(),
        }
    }

    /// True if every executed test passed (and at least the build succeeded).
    pub fn all_green(&self) -> bool {
        self.compiles && self.tests_failed == 0
    }

    /// Lexicographic comparison key: (compiles, -tests_failed, tests_passed, score).
    fn key(&self) -> (bool, i64, i64, f64) {
        (
            self.compiles,
            -(self.tests_failed as i64),
            self.tests_passed as i64,
            self.score,
        )
    }

    /// Strictly better than `other` under the lexicographic gate ordering.
    ///
    /// NaN scores are treated as the floor so a malformed measurement can never
    /// be judged an improvement.
    pub fn is_better_than(&self, other: &Fitness) -> bool {
        let (ac, af, ap, asc) = self.key();
        let (bc, bf, bp, bsc) = other.key();
        match (ac.cmp(&bc), af.cmp(&bf), ap.cmp(&bp)) {
            (std::cmp::Ordering::Greater, _, _) => true,
            (std::cmp::Ordering::Less, _, _) => false,
            (_, std::cmp::Ordering::Greater, _) => true,
            (_, std::cmp::Ordering::Less, _) => false,
            (_, _, std::cmp::Ordering::Greater) => true,
            (_, _, std::cmp::Ordering::Less) => false,
            _ => asc.partial_cmp(&bsc) == Some(std::cmp::Ordering::Greater),
        }
    }
}

/// Acceptance status of a variant after evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    /// Proposed but not yet evaluated.
    Pending,
    /// Evaluated and kept in the archive (it improved on its parent).
    Accepted,
    /// Evaluated and discarded (regression, build break, or no-op).
    Rejected,
}

/// One candidate self-modification with its lineage and measured fitness.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Variant {
    pub id: String,
    /// Archive id of the parent this variant was derived from (`None` for root).
    pub parent: Option<String>,
    /// 0 for the seed, incremented along each lineage.
    pub generation: u64,
    pub patch: Patch,
    /// Why the proposer believes this change helps.
    pub rationale: String,
    pub status: Status,
    pub fitness: Option<Fitness>,
    pub created_at: DateTime<Utc>,
}

impl Variant {
    /// The immutable root of the archive: the unmodified codebase, by definition
    /// the baseline every proposal is measured against.
    pub fn root(baseline: Fitness) -> Self {
        Self {
            id: new_id(),
            parent: None,
            generation: 0,
            patch: Patch::new("", "", ""),
            rationale: "baseline (unmodified codebase)".to_string(),
            status: Status::Accepted,
            fitness: Some(baseline),
            created_at: Utc::now(),
        }
    }

    /// A fresh, not-yet-evaluated child of `parent`.
    pub fn child(parent: &Variant, patch: Patch, rationale: impl Into<String>) -> Self {
        Self {
            id: new_id(),
            parent: Some(parent.id.clone()),
            generation: parent.generation + 1,
            patch,
            rationale: rationale.into(),
            status: Status::Pending,
            fitness: None,
            created_at: Utc::now(),
        }
    }
}

pub(crate) fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fit(compiles: bool, passed: u32, failed: u32, score: f64) -> Fitness {
        Fitness {
            compiles,
            tests_passed: passed,
            tests_failed: failed,
            score,
            notes: String::new(),
        }
    }

    #[test]
    fn compile_gate_dominates_score() {
        // A broken build with a huge score must lose to a working build.
        let broken = fit(false, 0, 0, 1_000.0);
        let working = fit(true, 1, 0, -1.0);
        assert!(working.is_better_than(&broken));
        assert!(!broken.is_better_than(&working));
    }

    #[test]
    fn test_regression_beats_score() {
        let regressed = fit(true, 10, 1, 100.0);
        let clean = fit(true, 5, 0, 0.0);
        assert!(clean.is_better_than(&regressed));
    }

    #[test]
    fn score_breaks_ties() {
        let lo = fit(true, 5, 0, 1.0);
        let hi = fit(true, 5, 0, 2.0);
        assert!(hi.is_better_than(&lo));
        assert!(!lo.is_better_than(&hi));
    }

    #[test]
    fn nan_score_is_never_an_improvement() {
        let nan = fit(true, 5, 0, f64::NAN);
        let ok = fit(true, 5, 0, 0.0);
        assert!(!nan.is_better_than(&ok));
    }

    #[test]
    fn noop_patch_detected() {
        assert!(Patch::new("a.rs", "x", "x").is_noop());
        assert!(!Patch::new("a.rs", "x", "y").is_noop());
    }
}
