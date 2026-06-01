//! # PPO — Proximal Policy Optimization
//!
//! L_CLIP = E[min(r_t A_t, clip(r_t, 1±ε) A_t)]
//!
//! Pour entraîner les paramètres de surface d'énergie via RL.

use crate::math::optimizer::Adam;
use serde::{Deserialize, Serialize};

/// Configuration PPO
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PPOConfig {
    /// Ratio de clipping ε
    pub clip_epsilon: f32,
    /// Coefficient de la loss de valeur
    pub value_coeff: f32,
    /// Coefficient d'entropie (encourage l'exploration)
    pub entropy_coeff: f32,
    /// Facteur de discount γ
    pub gamma: f32,
    /// GAE lambda λ
    pub gae_lambda: f32,
    /// Nombre d'époques par batch
    pub n_epochs: usize,
    /// Learning rate
    pub lr: f32,
}

impl Default for PPOConfig {
    fn default() -> Self {
        Self {
            clip_epsilon: 0.2,
            value_coeff: 0.5,
            entropy_coeff: 0.01,
            gamma: 0.99,
            gae_lambda: 0.95,
            n_epochs: 4,
            lr: 3e-4,
        }
    }
}

/// Expérience collectée (transition)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transition {
    /// État (vecteur)
    pub state: Vec<f32>,
    /// Action prise (index discret ou vecteur continu)
    pub action: usize,
    /// Récompense reçue
    pub reward: f32,
    /// Probabilité de l'action sous l'ancienne policy
    pub old_log_prob: f32,
    /// Estimation de la valeur de l'état
    pub value: f32,
    /// État terminal ?
    pub done: bool,
}

/// Buffer de rollout
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RolloutBuffer {
    pub transitions: Vec<Transition>,
    /// Avantages calculés (GAE)
    pub advantages: Vec<f32>,
    /// Returns (valeur cible)
    pub returns: Vec<f32>,
}

impl RolloutBuffer {
    pub fn new() -> Self {
        Self {
            transitions: Vec::new(),
            advantages: Vec::new(),
            returns: Vec::new(),
        }
    }

    pub fn push(&mut self, t: Transition) {
        self.transitions.push(t);
    }

    pub fn len(&self) -> usize {
        self.transitions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.transitions.is_empty()
    }

    pub fn clear(&mut self) {
        self.transitions.clear();
        self.advantages.clear();
        self.returns.clear();
    }

    /// Calculer les avantages GAE(γ, λ)
    ///
    /// A_t = Σ_{l=0}^{∞} (γλ)^l δ_{t+l}
    /// δ_t = r_t + γ V(s_{t+1}) - V(s_t)
    pub fn compute_gae(&mut self, gamma: f32, lambda: f32) {
        let n = self.transitions.len();
        self.advantages = vec![0.0; n];
        self.returns = vec![0.0; n];

        let mut gae = 0.0f32;

        for t in (0..n).rev() {
            let next_value = if t + 1 < n && !self.transitions[t].done {
                self.transitions[t + 1].value
            } else {
                0.0
            };

            let delta = self.transitions[t].reward
                + gamma * next_value
                - self.transitions[t].value;

            gae = delta + gamma * lambda * if self.transitions[t].done { 0.0 } else { gae };
            self.advantages[t] = gae;
            self.returns[t] = gae + self.transitions[t].value;
        }

        // Normaliser les avantages
        let mean: f32 = self.advantages.iter().sum::<f32>() / n as f32;
        let var: f32 = self.advantages.iter().map(|a| (a - mean).powi(2)).sum::<f32>() / n as f32;
        let std = var.sqrt().max(1e-8);
        for a in &mut self.advantages {
            *a = (*a - mean) / std;
        }
    }
}

/// Résultat d'une policy (distribution d'actions)
#[derive(Debug, Clone)]
pub struct PolicyOutput {
    /// Log-probabilités pour chaque action
    pub log_probs: Vec<f32>,
    /// Estimation de la valeur
    pub value: f32,
    /// Entropie de la distribution
    pub entropy: f32,
}

/// Policy simple : réseau linéaire softmax
/// (pour intégration avec le système d'agents)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinearPolicy {
    /// Poids de la policy (state_dim × n_actions)
    pub policy_weights: Vec<Vec<f32>>,
    /// Biais de la policy
    pub policy_bias: Vec<f32>,
    /// Poids de la value function (state_dim → 1)
    pub value_weights: Vec<f32>,
    /// Biais de la value function
    pub value_bias: f32,
}

impl LinearPolicy {
    pub fn new(state_dim: usize, n_actions: usize) -> Self {
        let scale = (2.0 / state_dim as f32).sqrt();
        let mut rng = rand::thread_rng();

        Self {
            policy_weights: (0..state_dim)
                .map(|_| (0..n_actions).map(|_| (rng.gen::<f32>() * 2.0 - 1.0) * scale).collect())
                .collect(),
            policy_bias: vec![0.0; n_actions],
            value_weights: (0..state_dim).map(|_| (rng.gen::<f32>() * 2.0 - 1.0) * scale).collect(),
            value_bias: 0.0,
        }
    }

    /// Forward : state → (log_probs, value, entropy)
    pub fn forward(&self, state: &[f32]) -> PolicyOutput {
        let n_actions = self.policy_bias.len();

        // Logits = state @ policy_weights + bias
        let mut logits = self.policy_bias.clone();
        for j in 0..n_actions {
            for i in 0..state.len().min(self.policy_weights.len()) {
                logits[j] += state[i] * self.policy_weights[i][j];
            }
        }

        // Log-softmax
        let max = logits.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let log_sum_exp = max + logits.iter().map(|x| (x - max).exp()).sum::<f32>().ln();
        let log_probs: Vec<f32> = logits.iter().map(|x| x - log_sum_exp).collect();

        // Entropie = -Σ p log p
        let entropy: f32 = -log_probs.iter()
            .map(|lp| lp.exp() * lp)
            .sum::<f32>();

        // Value = state @ value_weights + bias
        let value = self.value_bias + state.iter()
            .zip(self.value_weights.iter())
            .map(|(s, w)| s * w)
            .sum::<f32>();

        PolicyOutput { log_probs, value, entropy }
    }

    /// Collecte les paramètres en un vecteur plat (pour l'optimiseur)
    pub fn params_flat(&self) -> Vec<f32> {
        let mut params = Vec::new();
        for row in &self.policy_weights {
            params.extend(row);
        }
        params.extend(&self.policy_bias);
        params.extend(&self.value_weights);
        params.push(self.value_bias);
        params
    }

    /// Restore les paramètres depuis un vecteur plat
    pub fn set_params_flat(&mut self, flat: &[f32]) {
        let n_actions = self.policy_bias.len();
        let state_dim = self.policy_weights.len();
        let mut idx = 0;

        for i in 0..state_dim {
            for j in 0..n_actions {
                self.policy_weights[i][j] = flat[idx];
                idx += 1;
            }
        }
        for j in 0..n_actions {
            self.policy_bias[j] = flat[idx];
            idx += 1;
        }
        for i in 0..state_dim {
            self.value_weights[i] = flat[idx];
            idx += 1;
        }
        self.value_bias = flat[idx];
    }
}

use rand::Rng;

/// Calculer la loss PPO clippée pour un batch
///
/// L = -E[min(r_t A_t, clip(r_t, 1-ε, 1+ε) A_t)]
///     + c₁ (V - V_target)²
///     - c₂ H(π)
pub fn ppo_loss(
    buffer: &RolloutBuffer,
    policy: &LinearPolicy,
    config: &PPOConfig,
) -> f32 {
    let n = buffer.len();
    if n == 0 {
        return 0.0;
    }

    let mut total_policy_loss = 0.0f32;
    let mut total_value_loss = 0.0f32;
    let mut total_entropy = 0.0f32;

    for (i, t) in buffer.transitions.iter().enumerate() {
        let output = policy.forward(&t.state);
        let new_log_prob = output.log_probs[t.action];

        // r_t = π_new(a|s) / π_old(a|s) = exp(log_new - log_old)
        let ratio = (new_log_prob - t.old_log_prob).exp();

        let advantage = buffer.advantages[i];

        // Clipped surrogate
        let surr1 = ratio * advantage;
        let surr2 = ratio.clamp(1.0 - config.clip_epsilon, 1.0 + config.clip_epsilon) * advantage;
        total_policy_loss += -surr1.min(surr2);

        // Value loss
        let value_target = buffer.returns[i];
        total_value_loss += (output.value - value_target).powi(2);

        // Entropie
        total_entropy += output.entropy;
    }

    let n_f = n as f32;
    (total_policy_loss / n_f)
        + config.value_coeff * (total_value_loss / n_f)
        - config.entropy_coeff * (total_entropy / n_f)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gae_computation() {
        let mut buffer = RolloutBuffer::new();
        for i in 0..5 {
            buffer.push(Transition {
                state: vec![i as f32],
                action: 0,
                reward: 1.0,
                old_log_prob: -0.5,
                value: 0.5,
                done: i == 4,
            });
        }
        buffer.compute_gae(0.99, 0.95);
        assert_eq!(buffer.advantages.len(), 5);
        assert_eq!(buffer.returns.len(), 5);
    }

    #[test]
    fn test_policy_forward() {
        let policy = LinearPolicy::new(4, 3);
        let state = vec![1.0, 0.0, 0.5, -0.3];
        let output = policy.forward(&state);
        assert_eq!(output.log_probs.len(), 3);
        // Probas doivent sommer à ~1
        let sum: f32 = output.log_probs.iter().map(|lp| lp.exp()).sum();
        assert!((sum - 1.0).abs() < 1e-4);
    }
}
