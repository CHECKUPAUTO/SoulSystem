//! SoulLink Brain integration into the souls binary.
//!
//! Wires previously orphaned SoulLink brain crates into the autonomous entity
//! runtime: hierarchical memory, metacognition, and Tree-of-Thoughts reasoning.
//!
//! Since the mathematical system audit, also integrates:
//! - `soul-error-unifier`: unified global error E_t = Σ w_i·E_i
//! - `soul-intrinsic-motivation`: curiosity-driven autonomous goal generation

use anyhow::Result;
use soul_error_unifier::ErrorUnifier;
use soul_intrinsic_motivation::IntrinsicMotivation;
use soullink_autonomy::metacognition::MetaCognition;
use soullink_memory_hierarchy::{
    ConsolidationConfig, EpisodicConfig, HierarchicalMemory, MemoryEntry, MemoryLayer,
    SemanticConfig,
};
use soullink_reasoning::{ThoughtTree, TreeConfig};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tracing::info;

/// Encapsulates all wired SoulLink brain components,
/// including the unified error function and intrinsic motivation.
pub struct BrainMesh {
    /// Hierarchical memory (working → episodic → semantic).
    pub memory: Arc<HierarchicalMemory>,
    /// Tree-of-Thoughts reasoning engine.
    pub reasoning: ThoughtTree,
    /// Metacognition self-model.
    pub metacognition: Arc<MetaCognition>,
    /// Unified global error tracker E_t = Σ w_i·E_i (interior mutability for background task).
    pub error_unifier: Mutex<ErrorUnifier>,
    /// Curiosity-driven autonomous goal generator (interior mutability for background task).
    pub intrinsic_motivation: Mutex<IntrinsicMotivation>,
    /// Number of active HNN organs (from soullink-core dynamics).
    pub organ_count: usize,
    /// Brain is initialized and functional.
    pub initialized: bool,
    /// Cycle counter since last error adaptation.
    cycle_count: AtomicU64,
}

impl BrainMesh {
    pub fn new(working_memory_capacity: usize) -> Self {
        let memory = Arc::new(HierarchicalMemory::new(
            working_memory_capacity,
            EpisodicConfig::default(),
            SemanticConfig::default(),
            ConsolidationConfig::default(),
        ));
        let reasoning = ThoughtTree::new(TreeConfig::default());
        let metacognition = MetaCognition::new();
        let error_unifier = Mutex::new(ErrorUnifier::new());
        let intrinsic_motivation =
            Mutex::new(IntrinsicMotivation::new().with_thresholds(0.3, 0.4, 3, 10));

        info!(
            "BrainMesh initialized: memory + reasoning + metacognition + error_unifier + intrinsic_motivation"
        );

        Self {
            memory,
            reasoning,
            metacognition,
            error_unifier,
            intrinsic_motivation,
            organ_count: 6,
            initialized: true,
            cycle_count: AtomicU64::new(0),
        }
    }

    /// Feed an observation into the brain for learning.
    /// Also updates the unified error tracker with prediction and goal errors.
    pub async fn observe(&self, input: &str, result: &str, success: bool) {
        let importance = if success { 0.7 } else { 0.3 };
        let entry = MemoryEntry {
            id: uuid::Uuid::new_v4().to_string(),
            text: format!("Input: {} → Result: {}", input, result),
            created_at: chrono::Utc::now().to_rfc3339(),
            last_accessed: chrono::Utc::now().to_rfc3339(),
            access_count: 1,
            importance,
            layer: MemoryLayer::Episodic,
            tags: vec!["brain_mesh".into()],
            embedding: None,
            associations: vec![],
            metadata: std::collections::HashMap::new(),
        };
        self.memory.store(entry, MemoryLayer::Episodic).await;

        // Update metacognition
        self.metacognition
            .register_capability("observation", importance as f64)
            .await;
        self.metacognition
            .record_outcome("observation", success)
            .await;

        // Feed error signals into the unifier
        let prediction_error = if success { 0.0 } else { 0.8 };
        let goal_error = if success { 0.0 } else { 0.7 };
        let uncertainty = 0.3;

        if let Ok(mut unifier) = self.error_unifier.lock() {
            let initiative_gap = self
                .intrinsic_motivation
                .lock()
                .map(|m| m.initiative_gap())
                .unwrap_or(0.0);
            unifier.feed(
                prediction_error,
                0.0,
                goal_error,
                0.0,
                uncertainty,
                initiative_gap,
            );
        }
    }

    /// Get self-model from metacognition.
    pub async fn self_model(&self) -> soullink_autonomy::metacognition::SelfModel {
        self.metacognition.self_model().await
    }

    /// Run periodic maintenance on the brain.
    ///
    /// This is the main integration point for the mathematical system's
    /// complete loop. Each cycle:
    /// 1. Updates the unified error tracker with current component values
    /// 2. Computes intrinsic motivation initiative signal
    /// 3. Generates autonomous goals if curiosity warrants it
    /// 4. Logs diagnostic information
    pub async fn maintain(&self) -> Result<()> {
        let cycle = self.cycle_count.fetch_add(1, Ordering::Relaxed) + 1;

        // ── 1. Collect error signals from subsystems ──
        let self_model = self.metacognition.self_model().await;
        let prediction_error = (1.0 - self_model.overall_health).clamp(0.0, 1.0);
        let goal_error = 0.1;

        let (uncertainty, initiative_gap) = {
            let mot = self
                .intrinsic_motivation
                .lock()
                .map_err(|e| anyhow::anyhow!("IntrinsicMotivation lock poisoned: {}", e))?;
            let u = 1.0 - mot.knowledge.skill_coverage;
            let g = mot.initiative_gap();
            (u, g)
        };

        {
            let mut unifier = self
                .error_unifier
                .lock()
                .map_err(|e| anyhow::anyhow!("ErrorUnifier lock poisoned: {}", e))?;
            unifier.feed(
                prediction_error,
                0.05,
                goal_error,
                0.0,
                uncertainty,
                initiative_gap,
            );
        }

        // ── 2. Compute intrinsic motivation ──
        {
            let mut mot = self
                .intrinsic_motivation
                .lock()
                .map_err(|e| anyhow::anyhow!("IntrinsicMotivation lock poisoned: {}", e))?;
            let active_goals = mot.active_intrinsic_goals;
            let initiative = mot.compute_initiative(uncertainty, active_goals);

            // ── 3. Generate autonomous goals ──
            if initiative.should_generate_goal {
                if let Some(goal) = mot.generate_goal(uncertainty, active_goals) {
                    info!(
                        goal_id = %goal.id,
                        domain = %goal.domain,
                        priority = goal.priority,
                        source = %goal.source,
                        "BrainMesh: auto-generated intrinsic goal"
                    );
                }
            }
        }

        // ── 4. Adapt error weights periodically ──
        if cycle % 16 == 0 {
            if let Ok(unifier) = self.error_unifier.lock() {
                info!(
                    total = unifier.error.total,
                    ema = unifier.error.ema,
                    trending_up = unifier.error.trending_up,
                    "BrainMesh: periodic diagnostic — {}",
                    unifier.diagnostic()
                );
            }
        }

        // ── 5. Critical error alert ──
        if let Ok(unifier) = self.error_unifier.lock() {
            if unifier.is_critical() {
                info!("BrainMesh: CRITICAL error level — triggering adaptation signal");
            }
        }

        Ok(())
    }

    /// Ingest curiosity probes into the intrinsic motivation engine.
    pub fn ingest_curiosity_probes(&self, probes: &[(String, f64)]) {
        if let Ok(mut mot) = self.intrinsic_motivation.lock() {
            mot.ingest_probes(probes);
        }
    }

    /// Register that an exploration domain was successfully investigated.
    pub fn register_exploration(&self, domain: &str, familiarity: f64) {
        if let Ok(mut mot) = self.intrinsic_motivation.lock() {
            mot.knowledge.explore(domain, familiarity);
            mot.goal_completed();
        }
    }

    /// Get a summary of brain state for diagnostics.
    pub fn status(&self) -> serde_json::Value {
        let (error_total, error_ema, error_trending, diagnostic) =
            if let Ok(unifier) = self.error_unifier.lock() {
                (
                    unifier.error.total,
                    unifier.error.ema,
                    unifier.error.trending_up,
                    unifier.diagnostic(),
                )
            } else {
                (0.0, 0.0, false, "locked".to_string())
            };

        let (gaps, active_goals, max_goals, coverage) =
            if let Ok(mot) = self.intrinsic_motivation.lock() {
                (
                    mot.knowledge.gaps.len(),
                    mot.active_intrinsic_goals,
                    mot.max_active_intrinsic_goals,
                    mot.knowledge.skill_coverage,
                )
            } else {
                (0, 0, 0, 0.0)
            };

        serde_json::json!({
            "initialized": self.initialized,
            "organs": self.organ_count,
            "cycle": self.cycle_count.load(Ordering::Relaxed),
            "components": [
                "memory", "reasoning", "metacognition",
                "error_unifier", "intrinsic_motivation"
            ],
            "error": {
                "total": error_total,
                "ema": error_ema,
                "trending_up": error_trending,
            },
            "intrinsic_motivation": {
                "gaps": gaps,
                "active_goals": active_goals,
                "max_goals": max_goals,
                "skill_coverage": coverage,
            },
            "diagnostic": diagnostic,
        })
    }
}

impl Default for BrainMesh {
    fn default() -> Self {
        Self::new(100)
    }
}
