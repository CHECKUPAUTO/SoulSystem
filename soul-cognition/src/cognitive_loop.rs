//! The perceive → recall → decide → experiment → consolidate → reflect loop.
//!
//! This is the connective tissue, not a re-implementation: the experiment
//! phase delegates to `soul_rsi`'s `RsiEngine` (the Darwin Gödel Machine —
//! propose, evaluate empirically, keep only real improvements), recall/storage
//! go through the five-tier [`CognitiveMemory`], every action is checked by the
//! [`PermissionGate`], and when no external goal is pending the loop turns to
//! [`Curiosity`] for intrinsic motivation.
//!
//! The honesty invariants are upheld throughout: perception only ever produces
//! `Observed` facts, experiment outcomes are the `Observed` results of an
//! executed evaluation, and any impactful action must clear the Level-2 gate.

use crate::curiosity::{Curiosity, Probe, ScoredProbe};
use crate::gate::{Confirmation, Decision, GateError, PermissionGate};
use crate::memory::{CognitiveMemory, MemoryError, MemoryTier, Reconciliation, Record};
use crate::provenance::Tagged;
use soul_rsi::{Evaluator, Proposer, RsiEngine, StepOutcome};
use soul_skills::{Episode, Retention, Skill, ValidatedSkillLibrary};
use std::sync::RwLock;

/// What the agent chooses to do this cycle.
#[derive(Debug, Clone, PartialEq)]
pub enum Focus {
    /// Pursue an externally-provided goal.
    Goal(String),
    /// No external goal: explore the most novel probe (intrinsic motivation).
    Explore(ScoredProbe),
    /// Nothing pending and nothing novel enough — idle.
    Idle,
}

/// Coordinates the cognitive cycle over shared memory, a curiosity scorer and
/// the permission gate.
pub struct CognitiveLoop {
    gate: PermissionGate,
    curiosity: Curiosity,
    memory: CognitiveMemory,
    /// A self-evolving, *validated* skill library: successful episodes are
    /// induced into skills and retained only if they clear the gate (the moat
    /// over Hermes-Agent's ungated skill self-improvement). Interior-mutable so
    /// the loop keeps its shared-`&self` API.
    skills: RwLock<ValidatedSkillLibrary>,
}

impl Default for CognitiveLoop {
    fn default() -> Self {
        Self::new()
    }
}

impl CognitiveLoop {
    pub fn new() -> Self {
        Self {
            gate: PermissionGate::new(),
            curiosity: Curiosity::new(),
            memory: CognitiveMemory::new(),
            skills: RwLock::new(ValidatedSkillLibrary::new(soul_skills::builtin_skills())),
        }
    }

    /// Build with a pre-seeded curiosity model (known semantic embeddings).
    pub fn with_curiosity(curiosity: Curiosity) -> Self {
        Self {
            gate: PermissionGate::new(),
            curiosity,
            memory: CognitiveMemory::new(),
            skills: RwLock::new(ValidatedSkillLibrary::new(soul_skills::builtin_skills())),
        }
    }

    /// Access shared memory (to seed user facts, inspect tiers, etc.).
    pub fn memory(&self) -> &CognitiveMemory {
        &self.memory
    }

    /// Mutable access to the curiosity model (to record newly-learned probes).
    pub fn curiosity_mut(&mut self) -> &mut Curiosity {
        &mut self.curiosity
    }

    /// PERCEIVE. An observation is, by construction, an `Observed` fact: it came
    /// from an executed tool call / measurement. It is logged to both working
    /// (active context) and episodic (event history) memory.
    pub async fn perceive(&self, observation: impl Into<String>, importance: f32) {
        let obs = Tagged::observed(observation.into());
        // Episodic/working accept any provenance; these calls cannot fail.
        let _ = self
            .memory
            .remember(MemoryTier::Working, obs.clone(), importance)
            .await;
        let _ = self
            .memory
            .remember(MemoryTier::Episodic, obs, importance)
            .await;
    }

    /// RECALL relevant context across all five tiers.
    pub async fn recall(&self, query: &str, limit: usize) -> Vec<Record> {
        self.memory.recall(query, limit).await
    }

    /// RECALL with associative expansion (A-MEM): surfaces memories *linked* to
    /// the query's matches, not just literal hits.
    pub async fn recall_related(&self, query: &str, limit: usize, min_link: f64) -> Vec<Record> {
        self.memory.recall_associative(query, limit, min_link).await
    }

    /// LEARN a keyed fact, reconciled against what's already known
    /// (Mem0 ADD/UPDATE/NOOP) rather than blindly appended — the consistent,
    /// provenance-honest way to record long-lived facts such as operator
    /// preferences (`User`) or the current strategic focus (`Strategic`).
    /// Returns what the reconciliation decided.
    pub fn learn_fact(
        &self,
        tier: MemoryTier,
        key: &str,
        value: Tagged<String>,
        importance: f32,
    ) -> Result<Reconciliation, MemoryError> {
        self.memory.reconcile_set(tier, key, value, importance)
    }

    /// DECIDE this cycle's focus: an external goal takes precedence; otherwise
    /// pursue the most novel probe curiosity can find.
    pub fn decide(&self, goal: Option<&str>, probes: &[Probe], min_novelty: f32) -> Focus {
        if let Some(g) = goal {
            return Focus::Goal(g.to_string());
        }
        match self.curiosity.most_interesting(probes, min_novelty) {
            Some(p) => Focus::Explore(p),
            None => Focus::Idle,
        }
    }

    /// GATE. Authorize an action before it runs — the single Level-2 chokepoint.
    pub fn authorize(
        &self,
        command: &str,
        confirmation: Option<&Confirmation>,
    ) -> Result<Decision, GateError> {
        self.gate.authorize(command, confirmation)
    }

    /// EXPERIMENT. Drive one step of the supplied `soul_rsi` engine and record
    /// the empirical outcome into reflexive memory (the Reflexion signal).
    pub async fn experiment<P, E>(
        &self,
        engine: &mut RsiEngine<P, E>,
    ) -> soul_rsi::Result<StepOutcome>
    where
        P: Proposer,
        E: Evaluator,
    {
        let outcome = engine.step()?;
        let note = match &outcome {
            StepOutcome::NoProposal => "experiment: no proposal this step".to_string(),
            StepOutcome::Evaluated {
                variant_id,
                accepted,
                fitness,
                ..
            } => format!(
                "experiment: variant {} {} (score {:.3})",
                variant_id,
                if *accepted { "accepted" } else { "rejected" },
                fitness.score
            ),
        };
        // The outcome is an Observed measurement (build + tests were executed).
        let _ = self
            .memory
            .remember(MemoryTier::Reflexive, Tagged::observed(note), 0.6)
            .await;
        Ok(outcome)
    }

    /// CONSOLIDATE episodic → semantic. Returns how many clusters were promoted.
    pub async fn consolidate(&self) -> usize {
        self.memory.consolidate().await
    }

    /// REFLECT. Persist a lesson (Reflexion-style verbal reinforcement) into
    /// reflexive memory so future cycles can avoid repeating dead ends.
    pub async fn reflect(&self, lesson: Tagged<String>, importance: f32) {
        let _ = self
            .memory
            .remember(MemoryTier::Reflexive, lesson, importance)
            .await;
    }

    /// INDUCE a skill from a completed task: turn the episode into a reusable,
    /// validated skill (Voyager/SEVerA — kept only if it clears the validation
    /// gate). This crystallises the loop's successful work into a growing skill
    /// library, the structural advantage over ungated skill self-improvement.
    /// Returns what the library decided (added / reinforced / rejected / skipped).
    pub fn induce_skill(&self, episode: &Episode) -> Retention {
        self.skills.write().unwrap().offer(episode)
    }

    /// Snapshot the current validated skill library.
    pub fn skills(&self) -> Vec<Skill> {
        self.skills.read().unwrap().skills().to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curiosity::Probe;
    use crate::provenance::Provenance;
    use soul_rsi::{
        Archive, ClosureEvaluator, Fitness, ImprovementContext, Patch, Proposal, RsiConfig,
        RsiEngine, SplitMix64,
    };
    use std::path::Path;
    use tempfile::TempDir;

    // ── focus / gate (no RSI) ───────────────────────────────────────────

    #[test]
    fn decide_prefers_external_goal() {
        let lp = CognitiveLoop::new();
        let f = lp.decide(Some("ship gateway parity"), &[], 0.1);
        assert_eq!(f, Focus::Goal("ship gateway parity".into()));
    }

    #[test]
    fn decide_explores_when_idle_and_novel() {
        // No known embeddings → everything is novel.
        let lp = CognitiveLoop::new();
        let probes = vec![Probe::new("study HTTP/2 flow control", vec![1.0, 0.0])];
        match lp.decide(None, &probes, 0.5) {
            Focus::Explore(p) => assert_eq!(p.topic, "study HTTP/2 flow control"),
            other => panic!("expected Explore, got {other:?}"),
        }
    }

    #[test]
    fn decide_idle_when_nothing_novel() {
        let lp = CognitiveLoop::new();
        assert_eq!(lp.decide(None, &[], 0.5), Focus::Idle);
    }

    #[test]
    fn authorize_blocks_level_two() {
        let lp = CognitiveLoop::new();
        assert!(lp.authorize("rm -rf /", None).is_err());
        assert_eq!(lp.authorize("ls", None).unwrap(), Decision::Allow);
    }

    // ── perception / recall ─────────────────────────────────────────────

    #[tokio::test]
    async fn perceived_facts_are_observed_and_recallable() {
        let lp = CognitiveLoop::new();
        lp.perceive("gateway latency p99 = 8ms", 0.7).await;
        let hits = lp.recall("latency", 5).await;
        assert!(!hits.is_empty());
        assert!(hits.iter().all(|r| r.provenance == Provenance::Observed));
    }

    #[tokio::test]
    async fn learn_fact_reconciles_instead_of_duplicating() {
        let lp = CognitiveLoop::new();
        assert_eq!(
            lp.learn_fact(
                MemoryTier::User,
                "language",
                Tagged::observed("French".into()),
                0.9
            )
            .unwrap(),
            Reconciliation::Added
        );
        // Re-learning the same fact is a NoOp, not a duplicate.
        assert_eq!(
            lp.learn_fact(
                MemoryTier::User,
                "language",
                Tagged::observed("French".into()),
                0.9
            )
            .unwrap(),
            Reconciliation::NoOp
        );
        assert_eq!(lp.memory().snapshot(MemoryTier::User).len(), 1);
    }

    #[test]
    fn induce_skill_adds_validated_novel_skill() {
        use soul_skills::EpisodeAction;
        let lp = CognitiveLoop::new();
        let before = lp.skills().len(); // builtins
        let ep = Episode::new(
            "summarize the quarterly metrics report",
            vec![
                EpisodeAction::new("read_file", "read the metrics report"),
                EpisodeAction::new("write", "draft the summary"),
            ],
            true,
        );
        match lp.induce_skill(&ep) {
            Retention::Added(s) => assert!(s.name.contains("summarize")),
            other => panic!("expected Added, got {other:?}"),
        }
        assert_eq!(lp.skills().len(), before + 1);
    }

    #[test]
    fn induce_skill_reinforces_matching_builtin() {
        use soul_skills::EpisodeAction;
        let lp = CognitiveLoop::new();
        let before = lp.skills().len();
        // "review ..." matches the built-in code-review skill's trigger.
        let ep = Episode::new(
            "review the auth module thoroughly",
            vec![
                EpisodeAction::new("read_file", "read auth module"),
                EpisodeAction::new("grep", "scan for issues"),
            ],
            true,
        );
        assert!(matches!(lp.induce_skill(&ep), Retention::Reinforced(_)));
        // Reinforced in place — no new skill added.
        assert_eq!(lp.skills().len(), before);
    }

    #[tokio::test]
    async fn recall_related_surfaces_associations() {
        let lp = CognitiveLoop::new();
        lp.memory()
            .reconcile_set(
                MemoryTier::Strategic,
                "a",
                Tagged::observed("gateway deadlock root cause".into()),
                0.6,
            )
            .unwrap();
        lp.memory()
            .reconcile_set(
                MemoryTier::Strategic,
                "b",
                Tagged::observed("gateway latency tuning plan".into()),
                0.9,
            )
            .unwrap();
        let related = lp.recall_related("deadlock", 10, 0.1).await;
        let texts: Vec<&str> = related.iter().map(|r| r.text.as_str()).collect();
        assert!(texts.contains(&"gateway latency tuning plan"));
    }

    // ── experiment (reuses soul-rsi unchanged) ──────────────────────────

    struct Incrementer;
    impl Proposer for Incrementer {
        fn propose(
            &self,
            ctx: &ImprovementContext<'_>,
            _rng: &mut SplitMix64,
        ) -> soul_rsi::Result<Option<Proposal>> {
            let cur = read_level(ctx.workspace_root);
            Ok(Some(Proposal {
                patch: Patch::new(
                    "src/level.txt",
                    format!("level = {cur}"),
                    format!("level = {}", cur + 1),
                ),
                rationale: format!("raise level to {}", cur + 1),
            }))
        }
    }

    fn read_level(root: &Path) -> i64 {
        std::fs::read_to_string(root.join("src/level.txt"))
            .unwrap_or_default()
            .trim()
            .strip_prefix("level = ")
            .and_then(|s| s.parse().ok())
            .unwrap_or(-1)
    }

    fn toy_engine(
        ws: &Path,
    ) -> RsiEngine<Incrementer, ClosureEvaluator<impl Fn(&Path) -> Fitness>> {
        let baseline = Fitness {
            compiles: true,
            tests_passed: 1,
            tests_failed: 0,
            score: 0.0,
            notes: "baseline".into(),
        };
        let evaluator = ClosureEvaluator::new(|root: &Path| Fitness {
            compiles: true,
            tests_passed: 1,
            tests_failed: 0,
            score: read_level(root) as f64,
            notes: String::new(),
        });
        RsiEngine::new(
            Archive::with_root(baseline),
            Incrementer,
            evaluator,
            RsiConfig::new(ws, "raise the level"),
            1,
        )
    }

    #[tokio::test]
    async fn experiment_steps_rsi_and_logs_to_reflexive() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/level.txt"), "level = 0").unwrap();

        let lp = CognitiveLoop::new();
        let mut engine = toy_engine(dir.path());

        let outcome = lp.experiment(&mut engine).await.unwrap();
        assert!(outcome.accepted(), "raising the level should be accepted");

        // The empirical outcome was recorded to reflexive memory as Observed.
        let reflex = lp.memory().snapshot(MemoryTier::Reflexive);
        assert_eq!(reflex.len(), 1);
        assert!(reflex[0].text.contains("accepted"));
        assert_eq!(reflex[0].provenance, Provenance::Observed);

        // The live toy workspace is never mutated by the loop.
        assert_eq!(read_level(dir.path()), 0);
    }

    #[tokio::test]
    async fn reflect_persists_a_lesson() {
        let lp = CognitiveLoop::new();
        lp.reflect(
            Tagged::deduced(
                "blanket lint allows hid real warnings".into(),
                &[Provenance::Observed],
            ),
            0.8,
        )
        .await;
        let reflex = lp.memory().snapshot(MemoryTier::Reflexive);
        assert_eq!(reflex.len(), 1);
        assert_eq!(reflex[0].provenance, Provenance::Deduced);
    }
}
