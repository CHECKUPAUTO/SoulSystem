//! # Adam / AdamW Optimizer
//!
//! Remplace apply_reward() par un apprentissage adaptatif des paramètres α, μ, β.
//!
//! Adam: m_t = β₁ m_{t-1} + (1-β₁) g_t
//!       v_t = β₂ v_{t-1} + (1-β₂) g_t²
//!       θ_{t+1} = θ_t - lr * m̂_t / (√v̂_t + ε)
//!
//! AdamW: Ajoute weight decay découplé : θ_{t+1} -= lr * λ * θ_t

use serde::{Deserialize, Serialize};

/// Configuration de l'optimiseur
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdamConfig {
    /// Learning rate
    pub lr: f32,
    /// Momentum decay (β₁)
    pub beta1: f32,
    /// Variance decay (β₂)
    pub beta2: f32,
    /// Epsilon numérique
    pub epsilon: f32,
    /// Weight decay (λ) — 0.0 = Adam pur, >0 = AdamW
    pub weight_decay: f32,
    /// Gradient clipping (max norm), None = pas de clip
    pub grad_clip: Option<f32>,
}

impl Default for AdamConfig {
    fn default() -> Self {
        Self {
            lr: 1e-3,
            beta1: 0.9,
            beta2: 0.999,
            epsilon: 1e-8,
            weight_decay: 0.01,
            grad_clip: Some(1.0),
        }
    }
}

/// État de l'optimiseur Adam/AdamW par paramètre
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdamState {
    /// Premier moment (moyenne mobile du gradient)
    pub m: Vec<f32>,
    /// Second moment (moyenne mobile du gradient²)
    pub v: Vec<f32>,
    /// Pas de temps
    pub t: u64,
}

/// Optimiseur Adam/AdamW
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Adam {
    pub config: AdamConfig,
    pub state: AdamState,
}

impl Adam {
    /// Créer un nouvel optimiseur pour `dim` paramètres
    pub fn new(dim: usize, config: AdamConfig) -> Self {
        Self {
            config,
            state: AdamState {
                m: vec![0.0; dim],
                v: vec![0.0; dim],
                t: 0,
            },
        }
    }

    /// Adam standard
    pub fn standard(dim: usize) -> Self {
        Self::new(
            dim,
            AdamConfig {
                weight_decay: 0.0,
                ..Default::default()
            },
        )
    }

    /// AdamW avec weight decay
    pub fn adamw(dim: usize) -> Self {
        Self::new(dim, AdamConfig::default())
    }

    /// Un pas d'optimisation : θ ← θ - update(gradient)
    ///
    /// Retourne le vecteur de mise à jour (delta)
    pub fn step(&mut self, params: &mut [f32], gradients: &[f32]) -> Vec<f32> {
        assert_eq!(params.len(), gradients.len());
        assert_eq!(params.len(), self.state.m.len());

        let dim = params.len();
        self.state.t += 1;
        let _t = self.state.t as f32;

        // Gradient clipping
        let grads = if let Some(max_norm) = self.config.grad_clip {
            clip_grad_norm(gradients, max_norm)
        } else {
            gradients.to_vec()
        };

        // Bias correction factors
        let bc1 = 1.0 - self.config.beta1.powi(self.state.t as i32);
        let bc2 = 1.0 - self.config.beta2.powi(self.state.t as i32);

        let mut deltas = vec![0.0; dim];

        for i in 0..dim {
            // m_t = β₁ * m_{t-1} + (1 - β₁) * g_t
            self.state.m[i] =
                self.config.beta1 * self.state.m[i] + (1.0 - self.config.beta1) * grads[i];

            // v_t = β₂ * v_{t-1} + (1 - β₂) * g_t²
            self.state.v[i] = self.config.beta2 * self.state.v[i]
                + (1.0 - self.config.beta2) * grads[i] * grads[i];

            // Bias-corrected estimates
            let m_hat = self.state.m[i] / bc1;
            let v_hat = self.state.v[i] / bc2;

            // Adam update
            let adam_update = self.config.lr * m_hat / (v_hat.sqrt() + self.config.epsilon);

            // AdamW weight decay (découplé)
            let wd_update = self.config.lr * self.config.weight_decay * params[i];

            deltas[i] = adam_update + wd_update;
            params[i] -= deltas[i];
        }

        deltas
    }

    /// Reset l'état de l'optimiseur
    pub fn reset(&mut self) {
        self.state.m.fill(0.0);
        self.state.v.fill(0.0);
        self.state.t = 0;
    }

    /// Learning rate avec warm-up linéaire + cosine decay
    pub fn lr_schedule(&self, step: u64, warmup_steps: u64, total_steps: u64) -> f32 {
        if step < warmup_steps {
            // Warm-up linéaire
            self.config.lr * (step as f32 / warmup_steps as f32)
        } else {
            // Cosine decay
            let progress = (step - warmup_steps) as f32 / (total_steps - warmup_steps) as f32;
            self.config.lr * 0.5 * (1.0 + (std::f32::consts::PI * progress).cos())
        }
    }

    /// Ajuster le learning rate dynamiquement
    pub fn set_lr(&mut self, lr: f32) {
        self.config.lr = lr;
    }

    /// Nombre de pas effectués
    pub fn steps(&self) -> u64 {
        self.state.t
    }
}

/// Clip le gradient par norme L2
fn clip_grad_norm(grads: &[f32], max_norm: f32) -> Vec<f32> {
    let norm: f32 = grads.iter().map(|g| g * g).sum::<f32>().sqrt();
    if norm > max_norm {
        let scale = max_norm / norm;
        grads.iter().map(|g| g * scale).collect()
    } else {
        grads.to_vec()
    }
}

/// Optimiseur SGD avec momentum (pour comparaison / baseline)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SGD {
    pub lr: f32,
    pub momentum: f32,
    velocity: Vec<f32>,
}

impl SGD {
    pub fn new(dim: usize, lr: f32, momentum: f32) -> Self {
        Self {
            lr,
            momentum,
            velocity: vec![0.0; dim],
        }
    }

    pub fn step(&mut self, params: &mut [f32], gradients: &[f32]) {
        for i in 0..params.len() {
            self.velocity[i] = self.momentum * self.velocity[i] + gradients[i];
            params[i] -= self.lr * self.velocity[i];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adam_converges() {
        // Minimiser f(x) = x² → minimum à x=0
        let mut params = vec![5.0];
        let mut opt = Adam::standard(1);
        opt.config.lr = 0.1;

        for _ in 0..200 {
            let grad = vec![2.0 * params[0]]; // ∇f = 2x
            opt.step(&mut params, &grad);
        }

        assert!(
            params[0].abs() < 0.01,
            "Adam devrait converger vers 0, got {}",
            params[0]
        );
    }

    #[test]
    fn test_adamw_weight_decay() {
        let mut params = vec![10.0];
        let mut opt = Adam::adamw(1);
        opt.config.lr = 0.01;

        for _ in 0..100 {
            let grad = vec![0.0]; // pas de gradient, seul le weight decay agit
            opt.step(&mut params, &grad);
        }

        assert!(
            params[0].abs() < 10.0,
            "AdamW weight decay devrait réduire le param"
        );
    }
}
