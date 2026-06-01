use scirust_autodiff::Dual;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// R-STDP Optimizer using Dual numbers for forward-mode automatic differentiation.
///
/// Each synergy type keeps a learned coefficient updated via neuro-evolutionary
/// reward/punishment signals.
#[derive(Clone, Serialize, Deserialize)]
pub struct RSTDPOptimizer {
    coefficients: HashMap<String, f64>,
    lr: f64,
    momentum: f64,
    velocity: HashMap<String, f64>,
    history: Vec<FeedbackEntry>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct FeedbackEntry {
    pub synergy_type: String,
    pub reward: f64,
    pub timestamp: String,
    pub note: Option<String>,
}

impl RSTDPOptimizer {
    pub fn new(lr: f64, momentum: f64) -> Self {
        RSTDPOptimizer {
            coefficients: HashMap::new(),
            lr,
            momentum,
            velocity: HashMap::new(),
            history: Vec::new(),
        }
    }

    pub fn reinforce(&mut self, synergy_type: &str, reward: f64, note: Option<&str>) {
        let current = self.get_coefficient(synergy_type);

        let coef_dual = Dual::var(current);
        let lr_dual = Dual::primal(self.lr);
        let reward_dual = Dual::primal(reward);

        let mut update_dual = coef_dual + lr_dual * reward_dual;

        if self.momentum > 0.0 {
            let prev_v = self.velocity.get(synergy_type).copied().unwrap_or(0.0);
            let new_v = self.momentum * prev_v + self.lr * reward;
            let v_dual = Dual::primal(new_v);
            update_dual = coef_dual + v_dual;
            self.velocity.insert(synergy_type.to_string(), new_v);
        }

        let new_value = update_dual.val();
        let _gradient = update_dual.grad();
        let clamped = new_value.clamp(0.1, 3.0);

        self.coefficients.insert(synergy_type.to_string(), clamped);

        let timestamp = chrono_now_iso();
        self.history.push(FeedbackEntry {
            synergy_type: synergy_type.to_string(),
            reward,
            timestamp,
            note: note.map(|s| s.to_string()),
        });
    }

    pub fn get_coefficient(&self, synergy_type: &str) -> f64 {
        self.coefficients.get(synergy_type).copied().unwrap_or(1.0)
    }

    pub fn feedback_history(&self) -> &[FeedbackEntry] {
        &self.history
    }

    pub fn coefficients(&self) -> &HashMap<String, f64> {
        &self.coefficients
    }

    pub fn lr(&self) -> f64 {
        self.lr
    }

    pub fn momentum(&self) -> f64 {
        self.momentum
    }
}

fn chrono_now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

// ---------------------------------------------------------------------------
// Scoring functions
// ---------------------------------------------------------------------------

use crate::types::Synergy;

pub fn score_synergy(s: &Synergy, opt: &RSTDPOptimizer) -> f32 {
    let coef = opt.get_coefficient(s.synergy_type_label()) as f32;
    let base = s.auto_score * s.value_weight() * (1.0 + 0.20 * (s.priority as f32 - 1.0));
    (base * coef) / s.effort_factor().max(0.5)
}

pub fn rescore_batch(synergies: &mut [Synergy], opt: &RSTDPOptimizer) {
    for s in synergies.iter_mut() {
        s.adjusted_score = score_synergy(s, opt);
    }
}
