//! Synapse — connection between neurons
//! Extracted from main.rs for modularity.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use rand::Rng;

use crate::brain::NeuronIdx;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Synapse {
    pub source: NeuronIdx,
    pub target: NeuronIdx,
    pub weight: f32,
    pub hebbian_strength: f32,
    pub active: bool, // false = élagage logique (pas de suppression physique)
    /// STDP eligibility trace — decays each tick, bumped on pre-synaptic firing
    pub eligibility: f32,
    /// Per-synapse eligibility decay rate (0.0 = instant, 1.0 = never decays)
    pub eligibility_decay: f32,
    /// Dynamic extension fields for self-modification.
    #[serde(default)]
    pub extensions: HashMap<String, f64>,
}

impl Synapse {
    pub fn new(source: NeuronIdx, target: NeuronIdx) -> Self {
        let mut rng = rand::rng();
        Self {
            source,
            target,
            weight: rng.random_range(0.3..0.7),
            hebbian_strength: 1.0,
            active: true,
            eligibility: 0.0,
            eligibility_decay: 0.95,
            extensions: HashMap::new(),
        }
    }

    #[inline]
    pub fn strengthen(&mut self, delta: f32) {
        self.weight = (self.weight + delta).min(1.0);
        self.hebbian_strength += delta;
    }

    #[inline]
    pub fn weaken(&mut self, delta: f32) {
        self.weight = (self.weight - delta).max(0.0);
        if self.weight < 0.05 {
            self.active = false; // élagage logique
        }
    }

    /// Bump eligibility trace on pre-synaptic firing.
    /// Uses Oja-like bound to prevent unbounded growth.
    #[inline]
    pub fn bump_eligibility(&mut self) {
        self.eligibility = (self.eligibility + self.weight * (1.0 - self.eligibility)).min(1.0);
    }

    /// Decay eligibility trace each tick.
    #[inline]
    pub fn decay_eligibility(&mut self) {
        self.eligibility *= self.eligibility_decay;
    }

    /// Apply reward signal: strengthen proportionally to eligibility trace.
    /// Returns the amount of strengthening applied.
    #[inline]
    pub fn reward(&mut self, signal: f32) -> f32 {
        let delta = self.eligibility * signal;
        if delta > 0.001 {
            self.weight = (self.weight + delta).min(1.0);
            self.hebbian_strength += delta * 0.1;
            delta
        } else {
            0.0
        }
    }
} // end impl Synapse