//! # Langevin SDE : dp = -∇U dt + σ dW
//!
//! Intégrateur stochastique Euler-Maruyama pour la créativité.
//! Permet aux agents d'explorer l'espace d'énergie avec du bruit contrôlé.

use rand::Rng;
use serde::{Deserialize, Serialize};

/// Configuration de l'intégrateur Langevin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LangevinConfig {
    /// Pas de temps dt
    pub dt: f32,
    /// Coefficient de diffusion σ (intensité du bruit)
    pub sigma: f32,
    /// Coefficient de friction γ (overdamped si > 0)
    pub gamma: f32,
    /// Température du bain thermique (σ² = 2γkT)
    pub temperature: f32,
}

impl Default for LangevinConfig {
    fn default() -> Self {
        Self {
            dt: 0.01,
            sigma: 0.1,
            gamma: 1.0,
            temperature: 1.0,
        }
    }
}

impl LangevinConfig {
    /// Config calibrée : σ² = 2γkT (relation fluctuation-dissipation)
    pub fn thermalized(dt: f32, gamma: f32, temperature: f32) -> Self {
        let sigma = (2.0 * gamma * temperature).sqrt();
        Self {
            dt,
            sigma,
            gamma,
            temperature,
        }
    }
}

/// État de la particule Langevin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LangevinState {
    /// Position q (vecteur d'état)
    pub q: Vec<f32>,
    /// Moment p (pour Langevin non-overdamped)
    pub p: Vec<f32>,
    /// Énergie potentielle courante
    pub energy: f32,
    /// Pas de simulation
    pub step: u64,
}

/// Intégrateur Langevin
pub struct LangevinIntegrator {
    pub config: LangevinConfig,
}

impl LangevinIntegrator {
    pub fn new(config: LangevinConfig) -> Self {
        Self { config }
    }

    /// Euler-Maruyama overdamped (premier ordre)
    /// dq = -γ⁻¹ ∇U dt + σ dW
    ///
    /// `grad_u` : gradient de l'énergie potentielle au point q
    /// `state` : état actuel (modifié in-place)
    pub fn step_overdamped<F>(&self, state: &mut LangevinState, energy_fn: F)
    where
        F: Fn(&[f32]) -> (f32, Vec<f32>), // retourne (U, ∇U)
    {
        let mut rng = rand::thread_rng();
        let dim = state.q.len();
        let (energy, grad) = energy_fn(&state.q);
        state.energy = energy;

        let sqrt_dt = self.config.dt.sqrt();

        for i in 0..dim {
            // Bruit brownien : dW ~ N(0, dt)
            let dw: f32 = rng.gen::<f32>() * 2.0 - 1.0; // approximation uniforme
            let noise = self.config.sigma * sqrt_dt * dw;

            // Euler-Maruyama : q_{t+1} = q_t - (∇U/γ) dt + σ √dt ξ
            state.q[i] -= (grad[i] / self.config.gamma) * self.config.dt + noise;
        }

        state.step += 1;
    }

    /// Langevin complet (non-overdamped, second ordre)
    /// dq = p/m dt
    /// dp = -∇U dt - γp dt + σ dW
    pub fn step_full<F>(&self, state: &mut LangevinState, energy_fn: F, mass: f32)
    where
        F: Fn(&[f32]) -> (f32, Vec<f32>),
    {
        let mut rng = rand::thread_rng();
        let dim = state.q.len();

        // Assurer que p a la bonne taille
        if state.p.len() != dim {
            state.p = vec![0.0; dim];
        }

        let (energy, grad) = energy_fn(&state.q);
        state.energy = energy;

        let sqrt_dt = self.config.dt.sqrt();

        for i in 0..dim {
            let dw: f32 = rng.gen::<f32>() * 2.0 - 1.0;
            let noise = self.config.sigma * sqrt_dt * dw;

            // Mise à jour du moment
            state.p[i] -=
                grad[i] * self.config.dt + self.config.gamma * state.p[i] * self.config.dt - noise;

            // Mise à jour de la position
            state.q[i] += (state.p[i] / mass) * self.config.dt;
        }

        state.step += 1;
    }

    /// Simuler N pas et collecter la trajectoire
    pub fn simulate_trajectory<F>(
        &self,
        state: &mut LangevinState,
        energy_fn: F,
        n_steps: usize,
    ) -> Vec<(Vec<f32>, f32)>
    where
        F: Fn(&[f32]) -> (f32, Vec<f32>),
    {
        let mut trajectory = Vec::with_capacity(n_steps);

        for _ in 0..n_steps {
            self.step_overdamped(state, &energy_fn);
            trajectory.push((state.q.clone(), state.energy));
        }

        trajectory
    }

    /// Simulated annealing : diminuer σ au cours du temps
    pub fn anneal(&mut self, factor: f32) {
        self.config.sigma *= factor;
        self.config.temperature *= factor * factor; // T ∝ σ²
    }

    /// Énergie cinétique : Σ p²/(2m)
    pub fn kinetic_energy(state: &LangevinState, mass: f32) -> f32 {
        state.p.iter().map(|pi| pi * pi / (2.0 * mass)).sum()
    }

    /// Énergie totale : H = T + U
    pub fn total_energy(state: &LangevinState, mass: f32) -> f32 {
        Self::kinetic_energy(state, mass) + state.energy
    }
}

/// Créer un état initial
pub fn initial_state(q0: Vec<f32>) -> LangevinState {
    let dim = q0.len();
    LangevinState {
        q: q0,
        p: vec![0.0; dim],
        energy: 0.0,
        step: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_overdamped_converges_to_minimum() {
        // Potentiel harmonique U = 0.5 * |q|² → minimum à q=0
        let energy_fn = |q: &[f32]| {
            let u: f32 = q.iter().map(|x| 0.5 * x * x).sum();
            let grad: Vec<f32> = q.to_vec(); // ∇U = q
            (u, grad)
        };

        let config = LangevinConfig {
            dt: 0.01,
            sigma: 0.01, // peu de bruit
            gamma: 1.0,
            temperature: 0.001,
        };

        let integrator = LangevinIntegrator::new(config);
        let mut state = initial_state(vec![5.0, -3.0]);

        for _ in 0..5000 {
            integrator.step_overdamped(&mut state, energy_fn);
        }

        let dist: f32 = state.q.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!(
            dist < 1.0,
            "Devrait converger vers l'origine, dist={}",
            dist
        );
    }
}
