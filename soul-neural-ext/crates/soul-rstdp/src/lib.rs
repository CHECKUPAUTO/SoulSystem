//! R-STDP optimizer using Dual numbers for forward-mode automatic differentiation.
use scirust_autodiff::Dual;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
    pub fn new(lr: f64) -> Self {
        Self { coefficients: HashMap::new(), lr, momentum: 0.0, velocity: HashMap::new(), history: Vec::new() }
    }
    pub fn with_momentum(lr: f64, momentum: f64) -> Self {
        assert!((0.0..=1.0).contains(&momentum), "momentum must be in [0,1]");
        Self { coefficients: HashMap::new(), lr, momentum, velocity: HashMap::new(), history: Vec::new() }
    }
    pub fn reinforce(&mut self, synergy_type: &str, reward: f64, note: Option<&str>) {
        let coef = *self.coefficients.get(synergy_type).unwrap_or(&1.0);
        let c = Dual::var(coef);
        let delta = Dual::primal(self.lr * reward);
        let new_c = c + delta;
        let raw = new_c.val();
        let clamped = raw.clamp(0.1, 3.0);
        self.coefficients.insert(synergy_type.to_string(), clamped);
        let v = self.velocity.entry(synergy_type.to_string()).or_insert(0.0);
        *v = self.momentum * *v + (1.0 - self.momentum) * (clamped - coef);
        self.history.push(FeedbackEntry {
            synergy_type: synergy_type.to_string(),
            reward,
            timestamp: format!("{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()),
            note: note.map(String::from),
        });
    }
    pub fn get_coefficient(&self, synergy_type: &str) -> f64 {
        *self.coefficients.get(synergy_type).unwrap_or(&1.0)
    }
    pub fn feedback_history(&self) -> &[FeedbackEntry] { &self.history }
    pub fn coefficients(&self) -> &HashMap<String, f64> { &self.coefficients }
    pub fn momentum(&self) -> f64 { self.momentum }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_reinforce_increases_coefficient() {
        let mut opt = RSTDPOptimizer::new(0.1);
        opt.reinforce("Test", 1.0, None);
        assert!(opt.get_coefficient("Test") > 1.0);
    }
    #[test]
    fn test_reject_decreases_coefficient() {
        let mut opt = RSTDPOptimizer::new(0.1);
        opt.reinforce("Test", -1.0, None);
        assert!(opt.get_coefficient("Test") < 1.0);
    }
    #[test]
    fn test_coefficient_clamping() {
        let mut opt = RSTDPOptimizer::new(100.0);
        opt.reinforce("Test", 1.0, None);
        assert!(opt.get_coefficient("Test") <= 3.0);
        let mut opt2 = RSTDPOptimizer::new(100.0);
        opt2.coefficients.insert("Test".into(), 0.05);
        opt2.reinforce("Test", -1.0, None);
        assert!(opt2.get_coefficient("Test") >= 0.1);
    }
    #[test]
    fn test_dual_gradient() {
        let c = Dual::var(2.0);
        let delta = Dual::primal(0.5);
        let new_c = c + delta;
        assert!((new_c.grad() - 1.0).abs() < 1e-10, "grad = {}", new_c.grad());
    }
    #[test]
    fn test_feedback_history() {
        let mut opt = RSTDPOptimizer::new(0.1);
        opt.reinforce("A", 0.5, Some("test"));
        assert_eq!(opt.feedback_history().len(), 1);
    }
    #[test]
    fn test_default_coefficient_is_one() {
        let opt = RSTDPOptimizer::new(0.1);
        assert!((opt.get_coefficient("Unknown") - 1.0).abs() < 1e-10);
    }
    #[test]
    fn test_momentum_update() {
        let mut opt = RSTDPOptimizer::with_momentum(0.1, 0.9);
        opt.reinforce("M", 1.0, None);
        assert!(opt.get_coefficient("M") > 1.0);
    }
    #[test]
    fn test_serde_roundtrip() {
        let mut opt = RSTDPOptimizer::new(0.1);
        opt.reinforce("A", 0.5, None);
        let json = serde_json::to_string(&opt).unwrap();
        let opt2: RSTDPOptimizer = serde_json::from_str(&json).unwrap();
        assert!((opt2.get_coefficient("A") - opt.get_coefficient("A")).abs() < 1e-10);
    }
    #[test]
    fn test_coefficients_map() {
        let mut opt = RSTDPOptimizer::new(0.1);
        opt.reinforce("X", 0.5, None);
        assert!(opt.coefficients().contains_key("X"));
    }
}
