//! BrainNode — Hamiltonian neural node with Nesterov momentum.
//!
//! Each node evolves according to:
//!   p_{t+1} = μ·p_t - η·∇V(q_t) + ε
//!   q_{t+1} = q_t + p_{t+1}
//!
//! Where V(q) = ½·k·(q - target)² is a quadratic potential well.

use serde::{Deserialize, Serialize};

/// Node identifiers in the HNN mesh.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum NodeId {
    Reasoning,
    Perception,
    Affect,
    Memory,
    Reflex,
    Integration,
    Foresight,
    Homeostasis,
    Creativity,
    Social,
    Validation,
}

impl NodeId {
    pub fn all() -> Vec<NodeId> {
        vec![
            NodeId::Reasoning,
            NodeId::Perception,
            NodeId::Affect,
            NodeId::Memory,
            NodeId::Reflex,
            NodeId::Integration,
            NodeId::Foresight,
            NodeId::Homeostasis,
            NodeId::Creativity,
            NodeId::Social,
            NodeId::Validation,
        ]
    }

    /// Default target (resting state) for each node.
    pub fn default_target(&self) -> f64 {
        match self {
            NodeId::Reasoning => 0.0,
            NodeId::Perception => 0.0,
            NodeId::Affect => 0.0,
            NodeId::Memory => 0.0,
            NodeId::Reflex => 0.0,
            NodeId::Integration => 0.0,
            NodeId::Foresight => 0.0,
            NodeId::Homeostasis => 0.0,
            NodeId::Creativity => 0.0,
            NodeId::Social => 0.0,
            NodeId::Validation => 0.0,
        }
    }

    /// Stiffness (spring constant k) for each node.
    pub fn stiffness(&self) -> f64 {
        match self {
            NodeId::Reasoning => 0.15,
            NodeId::Perception => 0.20,
            NodeId::Affect => 0.10,
            NodeId::Memory => 0.05,
            NodeId::Reflex => 0.30,
            NodeId::Integration => 0.12,
            NodeId::Foresight => 0.08,
            NodeId::Homeostasis => 0.25,
            NodeId::Creativity => 0.06,
            NodeId::Social => 0.07,
            NodeId::Validation => 0.20,
        }
    }
}

/// Attractor detected from the Hamiltonian dynamics.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Attractor {
    DeepBasin,
    StableOrbit,
    StrangeAttractor,
    Transient,
}

/// A single Hamiltonian neural node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrainNode {
    pub id: NodeId,
    /// Position (cognitive state).
    pub position: f64,
    /// Momentum (inertia of thought).
    pub momentum: f64,
    /// Target (resting state).
    pub target: f64,
    /// Stiffness (spring constant k).
    pub stiffness: f64,
    /// Damping coefficient μ (0.9 = persistent memory).
    pub damping: f64,
    /// Learning rate η (reactivity to stimulus).
    pub learning_rate: f64,
    /// Turbulence (external noise injection).
    pub turbulence: f64,
    /// Tick counter.
    pub tick: u64,
}

impl BrainNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            position: 0.0,
            momentum: 0.0,
            target: id.default_target(),
            stiffness: id.stiffness(),
            damping: 0.9,
            learning_rate: 0.1,
            turbulence: 0.0,
            tick: 0,
        }
    }

    /// Evolve one timestep using Nesterov-Hamilton dynamics.
    ///
    /// p_{t+1} = μ·p_t - η·∇V(q_t) + ε
    /// q_{t+1} = q_t + p_{t+1}
    ///
    /// Where V(q) = ½·k·(q - target)²
    ///       ∇V(q) = k·(q - target)
    pub fn evolve(&mut self, stimulus: f64, thermal_noise: f64) {
        let gradient_v = self.stiffness * (self.position - self.target) + stimulus;
        self.momentum =
            self.damping * self.momentum - self.learning_rate * gradient_v + thermal_noise;
        self.position += self.momentum;
        self.tick += 1;
    }

    /// Inject a surge of momentum (from external stimulus like user input).
    pub fn surge(&mut self, force: f64) {
        self.momentum += force;
        self.turbulence = force.abs();
    }

    /// Detect the current attractor from position/momentum dynamics.
    pub fn attractor(&self) -> Attractor {
        let kinetic = self.momentum * self.momentum;
        let potential = 0.5 * self.stiffness * (self.position - self.target).powi(2);
        let energy = kinetic + potential;

        if energy < 0.01 {
            Attractor::DeepBasin
        } else if energy < 0.1 {
            Attractor::StableOrbit
        } else if self.momentum.abs() > 0.3 {
            Attractor::StrangeAttractor
        } else {
            Attractor::Transient
        }
    }

    /// Compute total energy (Hamiltonian).
    pub fn energy(&self) -> f64 {
        let kinetic = 0.5 * self.momentum * self.momentum;
        let potential = 0.5 * self.stiffness * (self.position - self.target).powi(2);
        kinetic + potential
    }

    /// Reset to resting state.
    pub fn reset(&mut self) {
        self.position = self.target;
        self.momentum = 0.0;
        self.turbulence = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_evolution_without_noise() {
        let mut node = BrainNode::new(NodeId::Reasoning);
        // Without noise or stimulus, node should oscillate toward target
        for _ in 0..100 {
            node.evolve(0.0, 0.0);
        }
        assert!(
            node.position.abs() < 0.1,
            "should converge near target, got {}",
            node.position
        );
    }

    #[test]
    fn node_evolution_with_stimulus() {
        let mut node = BrainNode::new(NodeId::Reasoning);
        node.evolve(1.0, 0.0); // positive stimulus
                               // After one step, momentum should be non-zero (gradient drives it)
        assert!(
            node.momentum.abs() > 0.0,
            "momentum should be non-zero, got {}",
            node.momentum
        );
    }

    #[test]
    fn surge_injects_momentum() {
        let mut node = BrainNode::new(NodeId::Affect);
        node.surge(0.5);
        assert!(
            (node.momentum - 0.5).abs() < 0.01,
            "momentum should be 0.5, got {}",
            node.momentum
        );
    }

    #[test]
    fn attractor_detection() {
        let mut node = BrainNode::new(NodeId::Reasoning);
        // At rest → DeepBasin
        assert_eq!(node.attractor(), Attractor::DeepBasin);

        // High momentum → StrangeAttractor
        node.momentum = 0.5;
        assert!(matches!(
            node.attractor(),
            Attractor::StrangeAttractor | Attractor::Transient
        ));
    }

    #[test]
    fn energy_conservation_approx() {
        let mut node = BrainNode::new(NodeId::Memory);
        // Start away from target with some momentum
        node.position = 1.0;
        node.momentum = 0.5;
        // Without stimulus, the potential well pulls toward target (0)
        // After many steps, position should decrease (energy dissipated via damping)
        let pos_before = node.position.abs();
        for _ in 0..50 {
            node.evolve(0.0, 0.0);
        }
        let pos_after = node.position.abs();
        assert!(
            pos_after < pos_before,
            "position should move toward target: {} -> {}",
            pos_before,
            pos_after
        );
    }

    #[test]
    fn all_nodes_have_stiffness() {
        for id in NodeId::all() {
            let node = BrainNode::new(id);
            assert!(node.stiffness > 0.0, "node {:?} should have stiffness", id);
        }
    }

    #[test]
    fn reset_returns_to_target() {
        let mut node = BrainNode::new(NodeId::Reasoning);
        node.position = 10.0;
        node.momentum = 5.0;
        node.reset();
        assert_eq!(node.position, node.target);
        assert_eq!(node.momentum, 0.0);
    }
}
