//! Intrinsic motivation — the bridge between curiosity and goal generation.
//!
//! Implements the mathematical system's component #8:
//!
//! ```text
//! I_t = f(U_t, K_t, G_t, D_t)
//! G_{t+1} = GenerateGoal(K_t, U_t, G_t)
//! ```
//!
//! Where:
//! - U_t: uncertainty (from curiosity module)
//! - K_t: knowledge state (what the agent knows)
//! - G_t: current goals (from goal tree)
//! - D_t: drive (exploration pressure from boredom)
//!
//! This crate closes the gap between SoulSystem's existing `Curiosity` module
//! (which scores probes by novelty) and the `GoalEngine` (which generates goals
//! reactively on heartbeat). With this bridge, the agent becomes *intrinsically
//! motivated* — it identifies knowledge gaps and autonomously creates goals to
//! fill them.
//!
//! # Architecture
//!
//! ```text
//! Curiosity (novelty scoring)
//!     │
//!     ▼
//! IntrinsicMotivation (gap identification + drive computation)
//!     │
//!     ▼
//! GoalEngine / GoalTree (goal creation + decomposition)
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::info;

// ─── Knowledge State ─────────────────────────────────────────────────────────

/// Tracks what the agent knows and where its knowledge gaps are.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeState {
    /// Known domains mapped to familiarity scores [0, 1].
    /// 1.0 = well-understood, 0.0 = completely unknown.
    pub known_regions: HashMap<String, f64>,

    /// Identified knowledge gaps worth exploring.
    pub gaps: Vec<KnowledgeGap>,

    /// Overall breadth of capabilities [0, 1].
    pub skill_coverage: f64,

    /// Total exploration actions performed.
    pub explorations: u64,
}

impl Default for KnowledgeState {
    fn default() -> Self {
        Self {
            known_regions: HashMap::new(),
            gaps: Vec::new(),
            skill_coverage: 0.1, // Start mostly unknown
            explorations: 0,
        }
    }
}

impl KnowledgeState {
    /// Register that a domain has been explored.
    pub fn explore(&mut self, domain: &str, familiarity: f64) {
        self.known_regions
            .insert(domain.to_string(), familiarity.clamp(0.0, 1.0));
        self.explorations += 1;

        // Update skill coverage: average of known regions
        if !self.known_regions.is_empty() {
            self.skill_coverage =
                self.known_regions.values().sum::<f64>() / self.known_regions.len() as f64;
        }

        // Remove satisfied gaps
        self.gaps.retain(|g| g.domain != domain);
    }

    /// Record a knowledge gap that hasn't been explored yet.
    pub fn record_gap(&mut self, domain: &str, current_knowledge: f64, information_value: f64) {
        if self.known_regions.contains_key(domain) {
            return; // Already explored
        }
        if self.gaps.iter().any(|g| g.domain == domain) {
            return; // Already recorded
        }
        self.gaps.push(KnowledgeGap {
            domain: domain.to_string(),
            current_knowledge: current_knowledge.clamp(0.0, 1.0),
            information_value: information_value.clamp(0.0, 1.0),
            reachable: true,
            recorded_at: chrono::Utc::now(),
        });
    }
}

// ─── Knowledge Gap ────────────────────────────────────────────────────────────

/// A specific domain where the agent's knowledge is insufficient.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeGap {
    /// Domain name (e.g. "system_performance", "security_config", "network_topology")
    pub domain: String,

    /// Current knowledge level [0, 1]
    pub current_knowledge: f64,

    /// Estimated information value of exploring this gap [0, 1]
    pub information_value: f64,

    /// Whether this gap can be explored with available tools.
    pub reachable: bool,

    /// When this gap was identified.
    pub recorded_at: chrono::DateTime<chrono::Utc>,
}

// ─── Initiative Signal ────────────────────────────────────────────────────────

/// The output of the intrinsic motivation computation.
/// Drives autonomous goal generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitiativeSignal {
    /// Overall intensity [0, 1]. High intensity → strong drive to create goals.
    pub intensity: f64,

    /// Current uncertainty from the curiosity module [0, 1].
    pub uncertainty: f64,

    /// Exploration drive (boredom pressure) [0, 1].
    pub drive: f64,

    /// Number of identified knowledge gaps.
    pub gap_count: usize,

    /// Whether goal generation should be triggered.
    pub should_generate_goal: bool,
}

// ─── Intrinsic Goal ───────────────────────────────────────────────────────────

/// A goal generated from intrinsic motivation (as opposed to external triggers).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntrinsicGoal {
    /// Unique goal ID.
    pub id: String,

    /// Human-readable description.
    pub description: String,

    /// Priority [0, 255], derived from information value.
    pub priority: u8,

    /// Source of this goal.
    pub source: GoalSource,

    /// Expected information gain from pursuing this goal [0, 1].
    pub expected_information_gain: f64,

    /// The domain this goal targets.
    pub domain: String,

    /// When this goal was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Distinguishes intrinsic goals from externally triggered ones.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum GoalSource {
    /// Generated by intrinsic motivation (curiosity-driven).
    Intrinsic,
    /// Generated reactively (heartbeat timer, LLM response, external event).
    Reactive,
    /// Generated from social learning (observing another agent).
    Social,
}

impl std::fmt::Display for GoalSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Intrinsic => write!(f, "intrinsic"),
            Self::Reactive => write!(f, "reactive"),
            Self::Social => write!(f, "social"),
        }
    }
}

// ─── Intrinsic Motivation Engine ──────────────────────────────────────────────

/// Bridges curiosity to goal generation.
///
/// Core loop:
/// 1. Query curiosity module for current uncertainty U_t
/// 2. Identify knowledge gaps in K_t
/// 3. Compute exploration drive D_t from boredom
/// 4. If intensity > threshold, generate intrinsic goal G_{t+1}
///
/// The generated goal flows into the planner and goal tree like any other goal,
/// subject to the same permission gate.
#[derive(Debug)]
pub struct IntrinsicMotivation {
    /// Knowledge state tracker.
    pub knowledge: KnowledgeState,

    /// Minimum novelty threshold for gap identification.
    /// Probes below this are considered "already explored".
    pub min_novelty: f64,

    /// Minimum intensity threshold for goal generation.
    /// Below this, the agent stays idle rather than exploring noise.
    pub min_intensity: f64,

    /// Maximum number of intrinsic goals to have active at once.
    pub max_active_intrinsic_goals: usize,

    /// How many intrinsic goals are currently active.
    pub active_intrinsic_goals: usize,

    /// Boredom counter: how many cycles without new exploration.
    boredom_ticks: u64,

    /// Boredom threshold: after this many idle cycles, drive increases.
    boredom_threshold: u64,
}

impl Default for IntrinsicMotivation {
    fn default() -> Self {
        Self {
            knowledge: KnowledgeState::default(),
            min_novelty: 0.3,
            min_intensity: 0.4,
            max_active_intrinsic_goals: 3,
            active_intrinsic_goals: 0,
            boredom_ticks: 0,
            boredom_threshold: 10,
        }
    }
}

impl IntrinsicMotivation {
    pub fn new() -> Self {
        Self::default()
    }

    /// Configure the motivation engine.
    pub fn with_thresholds(
        mut self,
        min_novelty: f64,
        min_intensity: f64,
        max_goals: usize,
        boredom_threshold: u64,
    ) -> Self {
        self.min_novelty = min_novelty;
        self.min_intensity = min_intensity;
        self.max_active_intrinsic_goals = max_goals;
        self.boredom_threshold = boredom_threshold;
        self
    }

    /// Compute the exploration drive from boredom.
    ///
    /// Drive increases when the agent has been idle (no new explorations)
    /// for longer than `boredom_threshold` cycles.
    pub fn compute_drive(&mut self) -> f64 {
        if self.boredom_ticks < self.boredom_threshold {
            self.boredom_ticks += 1;
            return self.boredom_ticks as f64 / self.boredom_threshold as f64;
        }
        // Beyond threshold, drive saturates at 1.0
        1.0
    }

    /// Reset boredom counter (called when a new exploration is performed).
    pub fn reset_boredom(&mut self) {
        self.boredom_ticks = 0;
    }

    /// Compute the initiative signal.
    ///
    /// ```text
    /// I_t = f(U_t, K_t, G_t, D_t)
    /// intensity = uncertainty * gap_factor * drive
    /// ```
    ///
    /// # Arguments
    /// * `uncertainty` - Current uncertainty U_t from curiosity module [0, 1]
    /// * `active_goal_count` - Number of currently active goals |G_t|
    ///
    /// # Returns
    /// An `InitiativeSignal` indicating whether and how strongly to generate goals.
    pub fn compute_initiative(
        &mut self,
        uncertainty: f64,
        active_goal_count: usize,
    ) -> InitiativeSignal {
        let drive = self.compute_drive();

        // Gap factor: more unexplored gaps → stronger initiative.
        // Uses a logarithmic-like curve: 1 gap = 0.33, 3 gaps = 0.60, 10 gaps = 0.83
        let gap_count = self.knowledge.gaps.len() as f64;
        let gap_factor = if gap_count == 0.0 {
            0.0
        } else {
            (gap_count / (gap_count + 2.0)).clamp(0.0, 1.0)
        };

        // Goal saturation: if many goals already active, reduce initiative
        let goal_penalty = if active_goal_count > 0 {
            1.0 / (1.0 + active_goal_count as f64)
        } else {
            1.0 // No active goals → full initiative
        };

        let intensity = (uncertainty * gap_factor * drive * goal_penalty).clamp(0.0, 1.0);

        let should_generate = intensity >= self.min_intensity
            && self.active_intrinsic_goals < self.max_active_intrinsic_goals
            && !self.knowledge.gaps.is_empty();

        InitiativeSignal {
            intensity,
            uncertainty,
            drive,
            gap_count: self.knowledge.gaps.len(),
            should_generate_goal: should_generate,
        }
    }

    /// Generate an intrinsic goal from the most valuable unexplored knowledge gap.
    ///
    /// ```text
    /// G_{t+1} = GenerateGoal(K_t, U_t, G_t)
    /// j* = argmax(information_value_j)
    /// ```
    ///
    /// Returns `None` if no gaps exist or intensity is too low.
    pub fn generate_goal(
        &mut self,
        uncertainty: f64,
        active_goal_count: usize,
    ) -> Option<IntrinsicGoal> {
        let signal = self.compute_initiative(uncertainty, active_goal_count);

        if !signal.should_generate_goal {
            return None;
        }

        // Select the gap with the highest information value
        let best_gap = self
            .knowledge
            .gaps
            .iter()
            .filter(|g| g.reachable)
            .max_by(|a, b| {
                a.information_value
                    .partial_cmp(&b.information_value)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })?;

        // Curiosity value: C_t = E_p × V_information
        // Here we approximate prediction error with uncertainty
        let curiosity_value = uncertainty * best_gap.information_value;

        let priority = ((curiosity_value * 255.0) as u8).max(1);

        let goal = IntrinsicGoal {
            id: uuid::Uuid::new_v4().to_string(),
            description: format!(
                "Explore domain: {} (value={:.2})",
                best_gap.domain, best_gap.information_value
            ),
            priority,
            source: GoalSource::Intrinsic,
            expected_information_gain: best_gap.information_value,
            domain: best_gap.domain.clone(),
            created_at: chrono::Utc::now(),
        };

        info!(
            domain = %best_gap.domain,
            priority = priority,
            intensity = signal.intensity,
            "IntrinsicMotivation: generated exploration goal"
        );

        self.active_intrinsic_goals += 1;
        self.reset_boredom();

        Some(goal)
    }

    /// Mark an intrinsic goal as completed, freeing a slot for new goals.
    pub fn goal_completed(&mut self) {
        self.active_intrinsic_goals = self.active_intrinsic_goals.saturating_sub(1);
    }

    /// Feed probe candidates from the curiosity module.
    /// High-novelty probes become knowledge gaps.
    pub fn ingest_probes(&mut self, probes: &[(String, f64)]) // (topic, novelty)
    {
        for (topic, novelty) in probes {
            if *novelty >= self.min_novelty {
                let info_value = *novelty; // Novelty ≈ information value for unexplored domains
                self.knowledge.record_gap(topic, 1.0 - novelty, info_value);
            }
        }
    }

    /// Get the current initiative gap for the error unifier.
    /// Initiative gap = 1 - (active_intrinsic_goals / max_active_intrinsic_goals)
    /// when gaps exist; 0 when fully explored.
    pub fn initiative_gap(&self) -> f64 {
        if self.knowledge.gaps.is_empty() {
            return 0.0; // No gaps → no initiative needed
        }
        let saturation =
            self.active_intrinsic_goals as f64 / self.max_active_intrinsic_goals as f64;
        1.0 - saturation // High saturation → low gap, low saturation → high gap
    }

    /// Get a diagnostic summary.
    pub fn diagnostic(&self) -> String {
        format!(
            "IntrinsicMotivation(gaps={}, active_goals={}/{}, coverage={:.2}, boredom={}/{})",
            self.knowledge.gaps.len(),
            self.active_intrinsic_goals,
            self.max_active_intrinsic_goals,
            self.knowledge.skill_coverage,
            self.boredom_ticks,
            self.boredom_threshold,
        )
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_motivation_starts_with_no_gaps() {
        let m = IntrinsicMotivation::new();
        assert_eq!(m.knowledge.gaps.len(), 0);
        assert_eq!(m.active_intrinsic_goals, 0);
    }

    #[test]
    fn ingest_probes_creates_gaps_for_novel_items() {
        let mut m = IntrinsicMotivation::new();
        m.ingest_probes(&[
            ("security".into(), 0.9),
            ("network".into(), 0.8),
            ("memory".into(), 0.1), // Below threshold
        ]);
        // "memory" should be filtered out (below 0.3)
        assert_eq!(m.knowledge.gaps.len(), 2);
        assert!(m.knowledge.gaps.iter().any(|g| g.domain == "security"));
        assert!(m.knowledge.gaps.iter().any(|g| g.domain == "network"));
    }

    #[test]
    fn no_goal_when_no_gaps() {
        let mut m = IntrinsicMotivation::new();
        let goal = m.generate_goal(0.8, 0);
        assert!(goal.is_none());
    }

    #[test]
    fn generates_goal_when_gaps_exist_and_uncertainty_high() {
        let mut m = IntrinsicMotivation::new();
        m.boredom_threshold = 1; // Immediate drive for testing
        m.min_intensity = 0.2; // Lower threshold for testing
        m.ingest_probes(&[("security".into(), 0.9), ("network".into(), 0.7)]);

        let goal = m.generate_goal(0.9, 0);
        assert!(goal.is_some());
        let g = goal.unwrap();
        assert_eq!(g.source, GoalSource::Intrinsic);
        assert_eq!(g.domain, "security"); // Highest information value
        assert!(g.priority > 0);
    }

    #[test]
    fn respects_max_active_goals() {
        let mut m = IntrinsicMotivation::new();
        m.boredom_threshold = 1; // Immediate drive for testing
        m.min_intensity = 0.2; // Lower threshold for testing
        m.max_active_intrinsic_goals = 2;
        m.ingest_probes(&[("a".into(), 0.9), ("b".into(), 0.8), ("c".into(), 0.7)]);

        // Generate 2 goals
        assert!(m.generate_goal(0.9, 0).is_some());
        assert!(m.generate_goal(0.9, 0).is_some());
        // Third should be blocked
        assert!(m.generate_goal(0.9, 0).is_none());
    }

    #[test]
    fn completing_goal_frees_slot() {
        let mut m = IntrinsicMotivation::new();
        m.boredom_threshold = 1; // Immediate drive for testing
        m.min_intensity = 0.2; // Lower threshold for testing
        m.ingest_probes(&[("a".into(), 0.9), ("b".into(), 0.8)]);

        assert!(m.generate_goal(0.9, 0).is_some());
        assert_eq!(m.active_intrinsic_goals, 1);
        m.goal_completed();
        assert_eq!(m.active_intrinsic_goals, 0);
        // Now can generate again
        assert!(m.generate_goal(0.9, 0).is_some());
    }

    #[test]
    fn boredom_increases_drive() {
        let mut m = IntrinsicMotivation::new();
        m.boredom_threshold = 3;

        assert_eq!(m.compute_drive(), 1.0 / 3.0);
        assert_eq!(m.compute_drive(), 2.0 / 3.0);
        assert_eq!(m.compute_drive(), 1.0); // Saturated
    }

    #[test]
    fn exploration_resets_boredom() {
        let mut m = IntrinsicMotivation::new();
        for _ in 0..5 {
            m.compute_drive();
        }
        assert_eq!(m.boredom_ticks, 5);
        m.reset_boredom();
        assert_eq!(m.boredom_ticks, 0);
    }

    #[test]
    fn exploring_domain_removes_gap() {
        let mut m = IntrinsicMotivation::new();
        m.ingest_probes(&[("security".into(), 0.9)]);
        assert_eq!(m.knowledge.gaps.len(), 1);
        m.knowledge.explore("security", 0.8);
        assert_eq!(m.knowledge.gaps.len(), 0);
        assert!(m.knowledge.known_regions.contains_key("security"));
    }

    #[test]
    fn skill_coverage_updates_on_exploration() {
        let mut m = IntrinsicMotivation::new();
        assert!((m.knowledge.skill_coverage - 0.1).abs() < 1e-9);
        m.knowledge.explore("security", 0.8);
        assert!((m.knowledge.skill_coverage - 0.8).abs() < 1e-9);
        m.knowledge.explore("network", 0.6);
        assert!((m.knowledge.skill_coverage - 0.7).abs() < 1e-9); // avg of 0.8 and 0.6
    }

    #[test]
    fn initiative_gap_reflects_saturation() {
        let mut m = IntrinsicMotivation::new();
        m.max_active_intrinsic_goals = 3;
        m.ingest_probes(&[("a".into(), 0.9)]);

        // No active goals, gaps exist → high gap
        assert!((m.initiative_gap() - 1.0).abs() < 1e-9);

        // Some active goals → lower gap
        m.active_intrinsic_goals = 2;
        assert!((m.initiative_gap() - (1.0 / 3.0)).abs() < 1e-9);

        // All slots filled → gap 0
        m.active_intrinsic_goals = 3;
        assert!((m.initiative_gap()).abs() < 1e-9);
    }

    #[test]
    fn goal_priority_derives_from_curiosity_value() {
        let mut m = IntrinsicMotivation::new();
        m.boredom_threshold = 1; // Immediate drive for testing
        m.min_intensity = 0.2; // Lower threshold for testing
        m.ingest_probes(&[("important".into(), 0.98)]);

        let goal = m.generate_goal(0.9, 0).unwrap();
        // C_t = 0.9 * 0.98 = 0.882 → priority = 0.882 * 255 ≈ 224
        assert!(goal.priority > 200);
    }

    #[test]
    fn duplicate_gaps_are_not_recorded() {
        let mut m = IntrinsicMotivation::new();
        m.ingest_probes(&[("security".into(), 0.9)]);
        m.ingest_probes(&[("security".into(), 0.8)]);
        assert_eq!(m.knowledge.gaps.len(), 1);
    }

    #[test]
    fn low_uncertainty_blocks_goal_generation() {
        let mut m = IntrinsicMotivation::new();
        m.ingest_probes(&[("a".into(), 0.9)]);
        // Very low uncertainty → intensity below threshold
        let goal = m.generate_goal(0.1, 0);
        assert!(goal.is_none());
    }

    #[test]
    fn initiative_signal_reflects_all_factors() {
        let mut m = IntrinsicMotivation::new();
        m.ingest_probes(&[("a".into(), 0.9), ("b".into(), 0.8), ("c".into(), 0.7)]);

        let signal = m.compute_initiative(0.8, 0);
        // intensity ≈ 0.8 * (3/10) * (1/3) * 1.0 ≈ 0.08
        // With 3 gaps, gap_factor = 0.3, uncertainty = 0.8, drive = 1/3 ≈ 0.33
        assert!(signal.intensity > 0.0);
        assert_eq!(signal.gap_count, 3);
    }
}
