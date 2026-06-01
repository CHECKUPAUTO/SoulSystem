//! Temporal decay — edges weaken over time, pruning below threshold.

use serde::{Deserialize, Serialize};

/// Configuration for edge-weight temporal decay.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecayConfig {
    /// Half-life in seconds (default: 7 days = 604800).
    pub half_life_secs: f64,
    /// Minimum weight before pruning (default: 0.01).
    pub min_weight: f32,
}

impl Default for DecayConfig {
    fn default() -> Self {
        Self {
            half_life_secs: 604_800.0, // 7 days
            min_weight: 0.01,
        }
    }
}

impl DecayConfig {
    /// Compute the decay multiplier for a given age.
    pub fn decay_factor(&self, age_secs: f64) -> f32 {
        0.5f64.powf(age_secs / self.half_life_secs) as f32
    }

    /// Apply decay to a weight and return `None` if below pruning threshold.
    pub fn apply(&self, weight: f32, age_secs: f64) -> Option<f32> {
        let decayed = weight * self.decay_factor(age_secs);
        if decayed < self.min_weight {
            None
        } else {
            Some(decayed)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decay_factor_fresh() {
        let cfg = DecayConfig::default();
        // Age 0 → factor 1.0
        let f = cfg.decay_factor(0.0);
        assert!((f - 1.0).abs() < 1e-6);
    }

    #[test]
    fn decay_factor_half_life() {
        let cfg = DecayConfig::default();
        // Age == half_life → factor ≈ 0.5
        let f = cfg.decay_factor(cfg.half_life_secs);
        assert!((f - 0.5).abs() < 1e-3);
    }

    #[test]
    fn decay_factor_old() {
        let cfg = DecayConfig::default();
        // Age = 10 × half_life → factor ≈ 0.001
        let f = cfg.decay_factor(cfg.half_life_secs * 10.0);
        assert!(f < 0.002);
    }

    #[test]
    fn apply_prunes_below_threshold() {
        let cfg = DecayConfig::default();
        // Very old edge with low weight → None
        assert!(cfg.apply(0.01, cfg.half_life_secs * 20.0).is_none());
    }

    #[test]
    fn apply_keeps_above_threshold() {
        let cfg = DecayConfig::default();
        // Recent edge with decent weight → Some
        assert!(cfg.apply(1.0, 100.0).is_some());
    }
}
