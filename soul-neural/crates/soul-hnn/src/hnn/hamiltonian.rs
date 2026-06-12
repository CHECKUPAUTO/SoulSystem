use super::integrator::SymplecticIntegrator;
use super::EnergyNetwork;

/// HamiltonianNN — wrapper around EnergyNetwork + SymplecticIntegrator
/// that maintains a combined state vector [q_0..q_n, p_0..p_n].
///
/// The state is a flat Vec<f64> of length 2×dim, where the first dim
/// entries are positions q_i and the last dim entries are momenta p_i.
#[derive(Debug, Clone)]
pub struct HamiltonianNN {
    state: Vec<f64>,
    dim: usize,
    energy: EnergyNetwork,
    integrator: SymplecticIntegrator,
}

impl HamiltonianNN {
    pub fn new(dim: usize) -> Self {
        let state = vec![0.0; 2 * dim];
        HamiltonianNN {
            dim,
            state,
            energy: EnergyNetwork::new(0.5, 0.1, 0.0),
            integrator: SymplecticIntegrator::new(0.01, 1),
        }
    }

    /// Dimensionality of the position/momentum subspace (dim, not 2×dim).
    pub fn state_dim(&self) -> usize {
        self.dim
    }

    /// Immutable borrow of the full state vector [q..; p..].
    pub fn get_state(&self) -> &[f64] {
        &self.state
    }

    /// Mutable borrow of the full state vector [q..; p..].
    pub fn get_state_mut(&mut self) -> &mut [f64] {
        &mut self.state
    }

    /// Apply an embedding impulse to the momentum components.
    pub fn apply_impulse(&mut self, embedding: &[f64]) {
        let n = self.dim;
        for (i, &val) in embedding.iter().enumerate() {
            if i < n {
                self.state[n + i] += val * 0.01;
            }
        }
    }

    /// Get a copy of the state vector.
    pub fn get_state_vector(&self) -> Vec<f64> {
        self.state.clone()
    }

    /// Step the dynamics by one integration step.
    pub fn step(&mut self) {
        let n = self.dim;
        let mut new_state = self.state.clone();
        for i in 0..n {
            let q = self.state[i];
            let p = self.state[n + i];
            // Single step of SymplecticIntegrator for each oscillator
            let traj = self.integrator.integrate(q, p, &self.energy);
            if let Some(&(q_new, p_new)) = traj.last() {
                new_state[i] = q_new;
                new_state[n + i] = p_new;
            }
        }
        self.state = new_state;
    }
}
