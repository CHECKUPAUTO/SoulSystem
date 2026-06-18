//! Unified global error function for the adaptive agent.
//!
//! Implements the mathematical system's component #12:
//!
//! ```text
//! E_t = w_p·E_p + w_r·δ_t + w_g·E_g + w_s·E_social + w_u·U_t + w_i·I_t
//! ```
//!
//! Where:
//! - E_p: prediction error (from HNN / meta_cortex / JEPA free energy)
//! - δ_t: reinforcement TD error (from Q-learning)
//! - E_g: goal error (from soul-critique weighted score)
//! - E_social: social error (from senate consensus divergence)
//! - U_t: uncertainty (from curiosity / entropy)
//! - I_t: initiative gap (reactive vs intrinsic goal generation)
//!
//! The unifier collects these six independent error signals — currently scattered
//! across the codebase — into a single scalar that drives cognitive adaptation
//! (θ_{t+1} = θ_t - α_θ·∇E_t) and self-improvement (A_{t+1} = A_t + β·Δ(A_t, E_t)).

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use tracing::info;

// ─── Error Components ─────────────────────────────────────────────────────────

/// Raw error signals collected from across the agent subsystems.
/// Each is normalized to [0, 1] where 0 = optimal, 1 = maximum error.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorComponents {
    /// Prediction error: how far the internal world model missed reality.
    /// Source: `meta_cortex.last_self_model_error` or `jepa.free_energy.prediction_error`
    pub prediction: f64,

    /// Reinforcement TD error: Q-learning temporal difference.
    /// Source: Q-learning `δ_t = r + γQ(s',a') - Q(s,a)`
    pub reinforcement: f64,

    /// Goal error: distance between expected goal and actual result.
    /// Source: `soul_critique.calculate_overall()` (lower score = higher error)
    pub goal: f64,

    /// Social error: consensus divergence across multi-agent deliberation.
    /// Source: `soullink_senate.agreement.consensus` (1 - consensus = error)
    pub social: f64,

    /// Uncertainty: entropy of the internal world model.
    /// Source: `soul_cognition.curiosity` or `meta_cortex.normalized_entropy`
    pub uncertainty: f64,

    /// Initiative gap: how much the agent relies on external triggers vs
    /// intrinsic motivation for goal generation.
    pub initiative: f64,
}

impl Default for ErrorComponents {
    fn default() -> Self {
        Self {
            prediction: 0.0,
            reinforcement: 0.0,
            goal: 0.0,
            social: 0.0,
            uncertainty: 0.0,
            initiative: 0.0,
        }
    }
}

impl ErrorComponents {
    /// Convert an `overall_score` (higher = better) into goal error (higher = worse).
    /// soul-critique scores are in [0, 10]; we invert to [0, 1].
    pub fn from_critique_score(score: f64) -> f64 {
        (1.0 - score / 10.0).clamp(0.0, 1.0)
    }

    /// Convert consensus (higher = more agreement) into social error (higher = more divergence).
    pub fn from_consensus(consensus: f64) -> f64 {
        (1.0 - consensus).clamp(0.0, 1.0)
    }

    /// Convert novelty (higher = more novel) into uncertainty complement.
    /// High novelty → high uncertainty about what's known.
    pub fn from_novelty(novelty: f64) -> f64 {
        novelty.clamp(0.0, 1.0)
    }
}

// ─── Error Weights ────────────────────────────────────────────────────────────

/// Configurable weights for each error component.
/// Default weights prioritize goal error and prediction error.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorWeights {
    /// Weight for prediction error (default: 0.20)
    pub w_prediction: f64,
    /// Weight for reinforcement error (default: 0.15)
    pub w_reinforcement: f64,
    /// Weight for goal error (default: 0.30)
    pub w_goal: f64,
    /// Weight for social error (default: 0.15)
    pub w_social: f64,
    /// Weight for uncertainty (default: 0.10)
    pub w_uncertainty: f64,
    /// Weight for initiative gap (default: 0.10)
    pub w_initiative: f64,
}

impl Default for ErrorWeights {
    fn default() -> Self {
        Self {
            w_prediction: 0.20,
            w_reinforcement: 0.15,
            w_goal: 0.30,
            w_social: 0.15,
            w_uncertainty: 0.10,
            w_initiative: 0.10,
        }
    }
}

impl ErrorWeights {
    pub fn validate(&self) -> bool {
        let sum = self.w_prediction
            + self.w_reinforcement
            + self.w_goal
            + self.w_social
            + self.w_uncertainty
            + self.w_initiative;
        (sum - 1.0).abs() < 0.01
    }
}

// ─── Global Error ─────────────────────────────────────────────────────────────

/// Computed global error with component breakdown and historical tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalError {
    /// Weighted sum of all error components: `E_t = Σ w_i·E_i`
    pub total: f64,

    /// Individual error components for diagnostics.
    pub components: ErrorComponents,

    /// Weights used for this computation.
    pub weights: ErrorWeights,

    /// Timestamp of this computation.
    pub timestamp_ms: u64,

    /// Historical error values for trend analysis (ring buffer, most recent last).
    #[serde(skip)]
    pub history: VecDeque<f64>,

    /// Exponential moving average for stability.
    pub ema: f64,

    /// Whether the error is trending up (worsening).
    pub trending_up: bool,
}

impl GlobalError {
    /// Compute the unified global error from raw component values.
    ///
    /// # Arguments
    /// * `components` - Raw error signals, each normalized to [0, 1]
    /// * `weights` - Configurable per-component weights
    ///
    /// # Returns
    /// A `GlobalError` with `total ∈ [0, 1]` where 0 = optimal, 1 = maximal error.
    pub fn compute(components: ErrorComponents, weights: &ErrorWeights) -> Self {
        let total = weights.w_prediction * components.prediction
            + weights.w_reinforcement * components.reinforcement
            + weights.w_goal * components.goal
            + weights.w_social * components.social
            + weights.w_uncertainty * components.uncertainty
            + weights.w_initiative * components.initiative;

        let total = total.clamp(0.0, 1.0);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let mut history = VecDeque::with_capacity(64);
        history.push_back(total);

        Self {
            total,
            components,
            weights: weights.clone(),
            timestamp_ms: now,
            history,
            ema: total,
            trending_up: false,
        }
    }

    /// Update with a new error computation, maintaining history and EMA.
    pub fn update(&mut self, components: ErrorComponents, weights: &ErrorWeights) {
        self.components = components;
        self.weights = weights.clone();

        self.total = weights.w_prediction * self.components.prediction
            + weights.w_reinforcement * self.components.reinforcement
            + weights.w_goal * self.components.goal
            + weights.w_social * self.components.social
            + weights.w_uncertainty * self.components.uncertainty
            + weights.w_initiative * self.components.initiative;

        self.total = self.total.clamp(0.0, 1.0);
        self.timestamp_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        // Update EMA: ema = α·current + (1-α)·previous, α = 0.1
        const ALPHA: f64 = 0.1;
        let prev_ema = self.ema;
        self.ema = ALPHA * self.total + (1.0 - ALPHA) * prev_ema;

        // Ring buffer: keep last 64 values
        if self.history.len() >= 64 {
            self.history.pop_front();
        }
        self.history.push_back(self.total);

        // Trend detection: last 5 values
        if self.history.len() >= 5 {
            let recent: Vec<f64> = self.history.iter().rev().take(5).copied().collect();
            self.trending_up =
                recent.first().copied().unwrap_or(0.0) > recent.last().copied().unwrap_or(0.0);
        }

        if self.total > 0.7 {
            info!(
                total = self.total,
                ema = self.ema,
                trending_up = self.trending_up,
                "Global error high — agent may need adaptation"
            );
        }
    }

    /// Adapt weights based on error component trends.
    /// Components that are consistently high get increased weight;
    /// components that are consistently low get decreased weight.
    /// Weights are re-normalized to sum to 1.0.
    pub fn adapt_weights(&mut self, history: &[f64; 6]) {
        // history = [pred_avg, reinf_avg, goal_avg, social_avg, uncert_avg, init_avg]
        let sum: f64 = history.iter().sum();
        if sum < 1e-9 {
            return; // No signal to adapt from
        }

        let new = ErrorWeights {
            w_prediction: history[0] / sum,
            w_reinforcement: history[1] / sum,
            w_goal: history[2] / sum,
            w_social: history[3] / sum,
            w_uncertainty: history[4] / sum,
            w_initiative: history[5] / sum,
        };

        self.weights = new;
    }

    /// Check if error EMA is critically high and requires immediate adaptation.
    pub fn is_critical(&self) -> bool {
        self.ema > 0.7
    }

    /// Get the gradient signal for cognitive adaptation.
    /// θ_{t+1} = θ_t - α_θ · ∇E_t where ∇E_t ≈ ΔE_t / Δt
    pub fn gradient(&self) -> Option<f64> {
        if self.history.len() < 2 {
            return None;
        }
        let prev = self.history[self.history.len() - 2];
        Some(self.total - prev) // positive = error increasing, negative = improving
    }

    /// Get a human-readable diagnostic summary.
    pub fn diagnostic(&self) -> String {
        let trend = if self.trending_up {
            "⬆ worsening"
        } else {
            "⬇ improving"
        };
        format!(
            "GlobalError(total={:.3}, ema={:.3}, trend={})\n  pred={:.3} reinf={:.3} goal={:.3} social={:.3} uncert={:.3} init={:.3}",
            self.total, self.ema, trend,
            self.components.prediction,
            self.components.reinforcement,
            self.components.goal,
            self.components.social,
            self.components.uncertainty,
            self.components.initiative,
        )
    }
}

impl Default for GlobalError {
    fn default() -> Self {
        Self::compute(ErrorComponents::default(), &ErrorWeights::default())
    }
}

// ─── Error Unifier (top-level orchestrator) ───────────────────────────────────

/// Top-level orchestrator that collects error signals from all subsystems
/// and produces a unified `GlobalError`.
#[derive(Debug, Default)]
pub struct ErrorUnifier {
    /// Current global error state.
    pub error: GlobalError,
    /// Running averages of each component for adaptive weighting.
    component_history: [f64; 6],
    /// Number of samples collected.
    sample_count: u64,
}

impl ErrorUnifier {
    pub fn new() -> Self {
        Self {
            error: GlobalError::default(),
            component_history: [0.0; 6],
            sample_count: 0,
        }
    }

    /// Feed all six error signals at once. Performs:
    /// 1. Normalize each component to [0, 1]
    /// 2. Compute weighted sum E_t = Σ w_i·E_i
    /// 3. Update EMA and trend
    /// 4. Periodically adapt weights (every 16 samples)
    pub fn feed(
        &mut self,
        prediction_error: f64,
        reinforcement_error: f64,
        goal_error: f64,
        social_error: f64,
        uncertainty: f64,
        initiative_error: f64,
    ) {
        let components = ErrorComponents {
            prediction: prediction_error.clamp(0.0, 1.0),
            reinforcement: reinforcement_error.clamp(0.0, 1.0),
            goal: goal_error.clamp(0.0, 1.0),
            social: social_error.clamp(0.0, 1.0),
            uncertainty: uncertainty.clamp(0.0, 1.0),
            initiative: initiative_error.clamp(0.0, 1.0),
        };

        // Update component running averages
        let n = self.sample_count.min(63) as f64;
        let alpha = 1.0 / (n + 1.0);
        self.component_history[0] =
            (1.0 - alpha) * self.component_history[0] + alpha * components.prediction;
        self.component_history[1] =
            (1.0 - alpha) * self.component_history[1] + alpha * components.reinforcement;
        self.component_history[2] =
            (1.0 - alpha) * self.component_history[2] + alpha * components.goal;
        self.component_history[3] =
            (1.0 - alpha) * self.component_history[3] + alpha * components.social;
        self.component_history[4] =
            (1.0 - alpha) * self.component_history[4] + alpha * components.uncertainty;
        self.component_history[5] =
            (1.0 - alpha) * self.component_history[5] + alpha * components.initiative;

        self.sample_count += 1;

        let weights = self.error.weights.clone();
        self.error.update(components, &weights);

        // Adapt weights every 16 samples based on component history
        if self.sample_count.is_multiple_of(16) {
            self.error.adapt_weights(&self.component_history);
            info!(
                weights = ?self.error.weights,
                "ErrorUnifier: adaptive weight rebalancing"
            );
        }
    }

    /// Get the gradient for cognitive parameter updates.
    /// θ_{t+1} = θ_t - α_θ · ∇E_t
    pub fn cognitive_gradient(&self) -> Option<f64> {
        self.error.gradient()
    }

    /// Check if error is critically high and requires immediate adaptation.
    pub fn is_critical(&self) -> bool {
        self.error.ema > 0.7
    }

    /// Get a diagnostic string for logging/monitoring.
    pub fn diagnostic(&self) -> String {
        self.error.diagnostic()
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_weights_sum_to_one() {
        assert!(ErrorWeights::default().validate());
    }

    #[test]
    fn zero_error_components_gives_zero_total() {
        let error = GlobalError::compute(ErrorComponents::default(), &ErrorWeights::default());
        assert!((error.total - 0.0).abs() < 1e-9);
        assert!(!error.trending_up);
    }

    #[test]
    fn max_error_gives_max_total() {
        let components = ErrorComponents {
            prediction: 1.0,
            reinforcement: 1.0,
            goal: 1.0,
            social: 1.0,
            uncertainty: 1.0,
            initiative: 1.0,
        };
        let error = GlobalError::compute(components, &ErrorWeights::default());
        assert!((error.total - 1.0).abs() < 1e-9);
    }

    #[test]
    fn weighted_components_contribute_proportionally() {
        let weights = ErrorWeights {
            w_goal: 1.0,
            w_prediction: 0.0,
            w_reinforcement: 0.0,
            w_social: 0.0,
            w_uncertainty: 0.0,
            w_initiative: 0.0,
        };
        let components = ErrorComponents {
            goal: 0.5,
            ..Default::default()
        };
        let error = GlobalError::compute(components, &weights);
        assert!((error.total - 0.5).abs() < 1e-9);
    }

    #[test]
    fn update_maintains_history() {
        let mut error = GlobalError::default();
        let components = ErrorComponents {
            prediction: 0.3,
            ..Default::default()
        };
        error.update(components, &ErrorWeights::default());
        assert_eq!(error.history.len(), 2); // initial + update
    }

    #[test]
    fn gradient_computed_from_history() {
        let mut error = GlobalError::default();
        // First update: set to 0.5
        let c1 = ErrorComponents {
            goal: 0.5,
            ..Default::default()
        };
        let w1 = ErrorWeights {
            w_goal: 1.0,
            ..ErrorWeights::default()
        };
        error.update(c1, &w1);

        // Second update: set to 0.3 (improving)
        let c2 = ErrorComponents {
            goal: 0.3,
            ..Default::default()
        };
        error.update(c2, &w1);

        let grad = error.gradient();
        assert!(grad.is_some());
        assert!(grad.unwrap() < 0.0, "error decreasing → negative gradient");
    }

    #[test]
    fn ema_tracks_with_smoothing() {
        let mut error = GlobalError::default();
        let weights = ErrorWeights {
            w_goal: 1.0,
            ..ErrorWeights::default()
        };

        let c = ErrorComponents {
            goal: 1.0,
            ..Default::default()
        };
        error.update(c, &weights);
        // EMA: 0.1 * 1.0 + 0.9 * 0.0 = 0.1
        assert!((error.ema - 0.1).abs() < 1e-9);
    }

    #[test]
    fn error_unifier_integrates_all_signals() {
        let mut unifier = ErrorUnifier::new();
        unifier.feed(0.1, 0.2, 0.3, 0.4, 0.5, 0.6);
        let expected = 0.20 * 0.1 + 0.15 * 0.2 + 0.30 * 0.3 + 0.15 * 0.4 + 0.10 * 0.5 + 0.10 * 0.6;
        assert!((unifier.error.total - expected).abs() < 1e-9);
    }

    #[test]
    fn weight_adaptation_redistributes() {
        let mut error = GlobalError::default();
        // All zero except uncertainty
        let history = [0.0, 0.0, 0.0, 0.0, 1.0, 0.0]; // only uncertainty has signal
        error.adapt_weights(&history);
        assert!(
            error.weights.w_uncertainty > 0.9,
            "uncertainty should dominate"
        );
    }

    #[test]
    fn is_critical_only_above_threshold() {
        let mut error = GlobalError::default();
        assert!(!error.is_critical());

        let weights = ErrorWeights {
            w_goal: 1.0,
            ..ErrorWeights::default()
        };
        let c = ErrorComponents {
            goal: 0.8,
            ..Default::default()
        };
        // EMA ≈ 0.1 * 0.8 = 0.08 after first update
        error.update(c.clone(), &weights);
        assert!(!error.is_critical());

        // After many updates, EMA approaches goal value
        for _ in 0..100 {
            error.update(c.clone(), &weights);
        }
        assert!(error.is_critical());
    }

    #[test]
    fn critique_score_inversion() {
        // score 10 → error 0, score 0 → error 1
        assert!((ErrorComponents::from_critique_score(10.0)).abs() < 1e-9);
        assert!((ErrorComponents::from_critique_score(0.0) - 1.0).abs() < 1e-9);
        assert!((ErrorComponents::from_critique_score(5.0) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn consensus_inversion() {
        assert!((ErrorComponents::from_consensus(1.0)).abs() < 1e-9);
        assert!((ErrorComponents::from_consensus(0.0) - 1.0).abs() < 1e-9);
    }
}
