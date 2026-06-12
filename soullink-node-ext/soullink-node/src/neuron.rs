//! Neuron — brain cell structure and dynamics
//! Extracted from main.rs for modularity.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use rand::Rng;

// Types re-exported from brain for convenience
use crate::brain::SynapseIdx;

#[inline]
fn now_f64() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

#[inline]
pub fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-10.0 * (x - 0.5)).exp())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Neuron {
    pub id: String,
    pub module: String,
    pub potential: f32,
    pub activation: f32,
    pub firing_threshold: f32,
    /// Indices dans Brain::synapses
    pub connections_out: Vec<SynapseIdx>,
    pub connections_in: Vec<SynapseIdx>,
    pub last_fired: f64,
    pub firing_count: u64,
    pub strength: f32,
    pub learning_rate: f32,
    pub consolidation_strength: f32,
    pub importance: f32,
    pub x: f32,
    pub y: f32,
    pub spike_time: f64,
    /// Metabolic energy reserve (0.0 = depleted, 1.0 = full).
    /// Decreases with firing and synaptic activity.
    /// Refills during low-activity periods (consolidation, night).
    pub energy_reserve: f32,
    /// Dynamic extension fields for self-modification (added at runtime via genome).
    #[serde(default)]
    pub extensions: HashMap<String, f64>,
}

impl Neuron {
    pub fn new(id: String, module: String, x: f32, y: f32) -> Self {
        let mut rng = rand::rng();
        Self {
            id,
            module,
            potential: rng.random_range(0.3..0.7),
            activation: 0.0,
            firing_threshold: 0.75 + rng.random_range(-0.1..0.1_f32),
            connections_out: Vec::new(),
            connections_in: Vec::new(),
            last_fired: 0.0,
            firing_count: 0,
            strength: rng.random_range(0.5..1.0),
            learning_rate: rng.random_range(0.1..0.3),
            consolidation_strength: 1.0,
            importance: rng.random_range(0.3..0.7),
            x,
            y,
            spike_time: 0.0,
            energy_reserve: rng.random_range(0.7..1.0),
            extensions: HashMap::new(),
        }
    }

    /// Mise à jour du potentiel membranaire (Euler simple).
    /// `tau` is the membrane time constant (0.9–0.999, evolved by genome).
    pub fn update(&mut self, synaptic_input: f32, external_input: f32, tau: f32) {
        let total = external_input + synaptic_input;
        self.potential = self.potential.mul_add(tau, total * (1.0 - tau));
        self.activation = sigmoid(self.potential);
    }

    /// Dépolarisation + reset.
    pub fn fire(&mut self) {
        let t = now_f64();
        self.potential = 0.0;
        self.last_fired = t;
        self.firing_count += 1;
        self.spike_time = t;
        self.importance = (self.importance + 0.03).min(1.0);
    }

    /// Consolidation = renforcement, jamais suppression.
    pub fn consolidate(&mut self, strength: f32) {
        self.consolidation_strength += strength * 0.1;
        self.importance = (self.importance + 0.02).min(1.0);
    }

    #[inline]
    pub fn is_spiking(&self) -> bool {
        self.spike_time > 0.0 && (now_f64() - self.spike_time) < 0.5
    }
} // end impl Neuron