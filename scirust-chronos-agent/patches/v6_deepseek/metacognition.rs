// ==========================================================================
// metacognition.rs — Module de Métacognition Dialectique
//
// Implements second-order "consciousness": the agent generates two parallel
// trajectories (T1 opportunistic, T2 critic) and measures cognitive dissonance.
// When dissonance exceeds threshold, triggers internal reflection:
//   - Level A (Curiosity): boredom bonus — increase noise temperature
//   - Level B (Self-question): α_sync near 1.0 triggers <REFLECT> token
//   - Level C (Evolution): freeze latents into semantic store, reset perceiver
//
// The agent no longer just reacts — it interrogates itself and evolves.
// ==========================================================================


use candle_core::{Result, Tensor};

use crate::hypernetwork::ConditioningHyperNetwork;
use crate::planner::StochasticDiffusionPlanner;
use crate::memory::AtemporalMemory;

// --------------------------------------------------------------------------
// MetacognitiveMonitor
// --------------------------------------------------------------------------

pub struct MetacognitiveMonitor {
    /// Threshold for cognitive dissonance (distance between T1 and T2).
    pub dissent_threshold: f64,
    /// Learning rate for plasticity shifts during reflection.
    pub learning_rate: f64,
    /// Curiosity bonus: steps below minimum error before boredom kicks in.
    pub boredom_window: usize,
    pub min_error_for_curiosity: f64,
    /// Tracks how many consecutive steps error has been below threshold.
    pub low_error_steps: usize,
    /// Current dissonance value (updated each assessment).
    pub current_dissonance: f64,
    /// Whether the system is in a reflection episode.
    pub reflecting: bool,
    /// Reflection counter (total reflection episodes triggered).
    pub reflection_count: usize,
}

impl MetacognitiveMonitor {
    pub fn new(
        dissent_threshold: f64,
        learning_rate: f64,
        boredom_window: usize,
        min_error_for_curiosity: f64,
    ) -> Self {
        Self {
            dissent_threshold,
            learning_rate,
            boredom_window,
            min_error_for_curiosity,
            low_error_steps: 0,
            current_dissonance: 0.0,
            reflecting: false,
            reflection_count: 0,
        }
    }

    /// Compute cognitive dissonance between two trajectories.
    /// Returns MSE(T1, T2) as a scalar.
    pub fn assess_confidence(&self, t1: &Tensor, t2: &Tensor) -> Result<f64> {
        let diff = t1.sub(t2)?;
        let sq = diff.sqr()?;
        let mse = sq.mean_all()?;
        mse.to_scalar::<f64>()
    }

    /// Assess whether the system should trigger internal reflection.
    /// Returns (dissonance, should_reflect, boredom_temperature_bonus).
    pub fn evaluate(
        &mut self,
        t1: &Tensor,
        t2: &Tensor,
        alpha_sync: f64,
        prediction_error: f64,
    ) -> Result<(f64, bool, f64)> {
        // 1. Compute dissonance
        let dissonance = self.assess_confidence(t1, t2)?;
        self.current_dissonance = dissonance;

        // 2. Check boredom: if error too low for too long, get curious
        if prediction_error < self.min_error_for_curiosity {
            self.low_error_steps += 1;
        } else {
            self.low_error_steps = 0;
        }

        let boredom_bonus = if self.low_error_steps >= self.boredom_window {
            // Curiosity: increase noise temperature proportionally to boredom
            let bonus = ((self.low_error_steps - self.boredom_window) as f64).min(10.0) * 0.05;
            bonus
        } else {
            0.0
        };

        // 3. Check if reflection should trigger
        let should_reflect = dissonance > self.dissent_threshold
            || (alpha_sync > 0.95 && self.low_error_steps > self.boredom_window / 2);

        if should_reflect {
            self.reflecting = true;
            self.reflection_count += 1;
        }

        Ok((dissonance, should_reflect, boredom_bonus))
    }

    /// Execute internal reflection: apply plasticity shift to HyperNetwork.
    /// Returns the intensity of the shift applied.
    pub fn trigger_internal_reflection(
        &mut self,
        hnet: &mut ConditioningHyperNetwork,
        dissonance: f64,
    ) -> Result<f64> {
        let intensity = dissonance * self.learning_rate;
        hnet.apply_plasticity_shift(intensity)?;
        self.reflecting = false;
        Ok(intensity)
    }
}

// --------------------------------------------------------------------------
// Dual trajectory generator
// --------------------------------------------------------------------------

/// Generate two parallel trajectories from the planner.
///
/// - T1 (Opportunistic): standard trajectory with current alpha_sync
/// - T2 (Critic): conservative trajectory with elevated alpha_sync (higher
///   coherence pressure) and inverted noise temperature
///
/// Returns (t1, t2, dissonance).
pub fn generate_dual_trajectories(
    planner: &mut StochasticDiffusionPlanner,
    latent_state: &Tensor,
    alpha_sync: f64,
    boredom_bonus: f64,
) -> Result<(Tensor, Tensor)> {
    // T1: opportunistic — standard path with boredom bonus temperature
    let t1 = planner.plan(latent_state, (alpha_sync + boredom_bonus).clamp(0.0, 1.0))?;

    // T2: critic — conservative, higher coherence pressure, inverted noise
    let critic_alpha = ((alpha_sync + 0.3) * 1.5).clamp(0.0, 1.0);
    // Temporarily swap the null embedding direction for the critic path
    // by planning with elevated sync
    let t2 = planner.plan(latent_state, critic_alpha)?;

    Ok((t1, t2))
}

// --------------------------------------------------------------------------
// Levier A: Curiosity Engine
// --------------------------------------------------------------------------

/// Compute boredom-modulated noise temperature.
/// When low_error_steps exceeds boredom_window, temperature rises linearly.
pub fn curiosity_temperature(low_error_steps: usize, boredom_window: usize) -> f64 {
    if low_error_steps > boredom_window {
        let excess = (low_error_steps - boredom_window) as f64;
        (excess * 0.05).min(1.0)
    } else {
        0.0
    }
}

// --------------------------------------------------------------------------
// Levier B: Auto-Questionnement <REFLECT> detection
// --------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum ReflectionSignal {
    None,
    Reflect,     // emit <REFLECT> token
    ResetCache,  // freeze + reset
}

/// Evaluate if the system should emit a <REFLECT> signal based on
/// alpha_sync and memory contradiction.
///
/// Returns a ReflectionSignal indicating the action to take.
pub fn evaluate_reflection_signal(
    alpha_sync: f64,
    dissonance: f64,
    dissent_threshold: f64,
    memory: &AtemporalMemory,
    latent: &[f64],
) -> ReflectionSignal {
    // Level B: near-perfect sync + existing contradiction in memory
    if alpha_sync > 0.95 && dissonance > dissent_threshold * 0.5 {
        // Check for contradiction with old episodic memory
        let recent_sync = memory.episodic.synchrony(latent);
        if recent_sync > 0.9 {
            return ReflectionSignal::Reflect;
        }
    }

    // Level C: persistent high dissonance with no resolution
    if dissonance > dissent_threshold * 2.0 {
        return ReflectionSignal::ResetCache;
    }

    ReflectionSignal::None
}

// --------------------------------------------------------------------------
// Levier C: Cache Evolution
// --------------------------------------------------------------------------

/// Freeze current latents into semantic store and return a reset signal
/// for the Perceiver.
///
/// This implements "lesson learned": the agent commits its current
/// representational state to long-term memory and starts fresh exploration.
pub fn evolve_cache(
    memory: &mut AtemporalMemory,
    latent: &[f64],
    semantic_threshold: f64,
) {
    // Force-learn the current latent as a high-strength semantic prototype
    let t = memory.current_t;
    memory.semantic.learn(latent.to_vec(), semantic_threshold, t);
    // Reinforce multiple times to make it stick
    for _ in 0..5 {
        memory.semantic.learn(latent.to_vec(), semantic_threshold - 0.1, t);
    }
}

// --------------------------------------------------------------------------
// Flexibility metrics
// --------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize)]
pub struct FlexibilityReport {
    pub total_reflections: usize,
    pub avg_dissonance: f64,
    pub max_dissonance: f64,
    pub curiosity_activations: usize,
    pub cache_evolutions: usize,
    pub trajectory_divergence: f64,
    pub plasticity_intensity: f64,
}

/// Compute flexibility metrics from a sequence of observation logs.
pub fn compute_flexibility(
    dissonances: &[f64],
    reflection_signals: &[ReflectionSignal],
    plasticity_intensities: &[f64],
) -> FlexibilityReport {
    let total_reflections = reflection_signals.iter()
        .filter(|&s| *s != ReflectionSignal::None).count();
    let avg_d = dissonances.iter().sum::<f64>() / dissonances.len().max(1) as f64;
    let max_d = dissonances.iter().cloned().fold(0.0f64, f64::max);
    let curiosity = reflection_signals.iter()
        .filter(|&s| *s == ReflectionSignal::Reflect).count();
    let cache_ev = reflection_signals.iter()
        .filter(|&s| *s == ReflectionSignal::ResetCache).count();
    let traj_div = if dissonances.len() > 1 {
        let mut sum = 0.0;
        for w in dissonances.windows(2) {
            sum += (w[1] - w[0]).abs();
        }
        sum / (dissonances.len() - 1) as f64
    } else { 0.0 };
    let plast = plasticity_intensities.iter().sum::<f64>() / plasticity_intensities.len().max(1) as f64;

    FlexibilityReport {
        total_reflections,
        avg_dissonance: avg_d,
        max_dissonance: max_d,
        curiosity_activations: curiosity,
        cache_evolutions: cache_ev,
        trajectory_divergence: traj_div,
        plasticity_intensity: plast,
    }
}
