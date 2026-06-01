//! # Mixture of Experts (MoE) : g(x) = softmax(W_g x) top-k
//!
//! Multi-basin surface — chaque expert = un bassin d'attraction.
//! Le gating network route les entrées vers les k meilleurs experts.

use crate::math::attention::softmax;
use rand::Rng;
use serde::{Deserialize, Serialize};

/// Configuration MoE
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoEConfig {
    /// Dimension d'entrée
    pub input_dim: usize,
    /// Dimension de sortie (par expert)
    pub output_dim: usize,
    /// Nombre d'experts
    pub n_experts: usize,
    /// Nombre d'experts activés (top-k)
    pub top_k: usize,
    /// Coefficient de balance loss (pour éviter le collapse sur un expert)
    pub balance_coeff: f32,
}

/// Expert individuel (réseau linéaire + ReLU)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Expert {
    pub weights: Vec<Vec<f32>>,
    pub bias: Vec<f32>,
    /// Compteur d'utilisation (pour la balance loss)
    pub usage_count: u64,
}

impl Expert {
    pub fn new(input_dim: usize, output_dim: usize) -> Self {
        let scale = (2.0 / input_dim as f32).sqrt();
        let mut rng = rand::thread_rng();

        Self {
            weights: (0..input_dim)
                .map(|_| (0..output_dim).map(|_| (rng.gen::<f32>() * 2.0 - 1.0) * scale).collect())
                .collect(),
            bias: vec![0.0; output_dim],
            usage_count: 0,
        }
    }

    /// Forward : ReLU(x @ W + b)
    pub fn forward(&self, x: &[f32]) -> Vec<f32> {
        let mut out = self.bias.clone();
        for j in 0..out.len() {
            for i in 0..x.len().min(self.weights.len()) {
                out[j] += x[i] * self.weights[i][j];
            }
            out[j] = out[j].max(0.0); // ReLU
        }
        out
    }

    /// Collecte les paramètres à plat
    pub fn params_flat(&self) -> Vec<f32> {
        let mut p = Vec::new();
        for row in &self.weights {
            p.extend(row);
        }
        p.extend(&self.bias);
        p
    }
}

/// Gating network
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatingNetwork {
    /// Poids du gating (input_dim × n_experts)
    pub weights: Vec<Vec<f32>>,
    pub bias: Vec<f32>,
    /// Bruit pour l'exploration (Noisy top-k)
    pub noise_scale: f32,
}

impl GatingNetwork {
    pub fn new(input_dim: usize, n_experts: usize) -> Self {
        let scale = (1.0 / input_dim as f32).sqrt();
        let mut rng = rand::thread_rng();

        Self {
            weights: (0..input_dim)
                .map(|_| (0..n_experts).map(|_| (rng.gen::<f32>() * 2.0 - 1.0) * scale).collect())
                .collect(),
            bias: vec![0.0; n_experts],
            noise_scale: 0.1,
        }
    }

    /// Calcule les logits du gating : g(x) = W_g x + b + noise
    pub fn compute_logits(&self, x: &[f32], add_noise: bool) -> Vec<f32> {
        let mut rng = rand::thread_rng();
        let n = self.bias.len();
        let mut logits = self.bias.clone();

        for j in 0..n {
            for i in 0..x.len().min(self.weights.len()) {
                logits[j] += x[i] * self.weights[i][j];
            }
            if add_noise {
                logits[j] += rng.gen::<f32>() * self.noise_scale;
            }
        }

        logits
    }

    /// Top-k gating : retourne les indices et poids des k meilleurs experts
    pub fn top_k_gate(&self, x: &[f32], k: usize, add_noise: bool) -> Vec<(usize, f32)> {
        let logits = self.compute_logits(x, add_noise);
        let probs = softmax(&logits);

        // Trouver top-k
        let mut indexed: Vec<(usize, f32)> = probs.iter().enumerate().map(|(i, &p)| (i, p)).collect();
        indexed.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        indexed.truncate(k);

        // Renormaliser les poids du top-k
        let sum: f32 = indexed.iter().map(|(_, w)| w).sum();
        if sum > 1e-10 {
            indexed.iter_mut().for_each(|(_, w)| *w /= sum);
        }

        indexed
    }
}

/// Mixture of Experts complet
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MixtureOfExperts {
    pub config: MoEConfig,
    pub experts: Vec<Expert>,
    pub gating: GatingNetwork,
}

impl MixtureOfExperts {
    pub fn new(config: MoEConfig) -> Self {
        let experts = (0..config.n_experts)
            .map(|_| Expert::new(config.input_dim, config.output_dim))
            .collect();
        let gating = GatingNetwork::new(config.input_dim, config.n_experts);

        Self { config, experts, gating }
    }

    /// Forward pass : route vers top-k experts et combine les sorties
    pub fn forward(&mut self, x: &[f32]) -> MoEOutput {
        let gates = self.gating.top_k_gate(x, self.config.top_k, true);

        let mut output = vec![0.0; self.config.output_dim];
        let mut expert_outputs = Vec::new();

        for &(expert_idx, weight) in &gates {
            let expert_out = self.experts[expert_idx].forward(x);
            self.experts[expert_idx].usage_count += 1;

            for j in 0..self.config.output_dim {
                output[j] += weight * expert_out[j];
            }

            expert_outputs.push((expert_idx, weight, expert_out));
        }

        MoEOutput {
            output,
            expert_outputs,
            balance_loss: self.compute_balance_loss(),
        }
    }

    /// Balance loss = coefficient de variation de l'utilisation des experts
    /// Pénalise le collapse où un seul expert est utilisé
    fn compute_balance_loss(&self) -> f32 {
        let counts: Vec<f32> = self.experts.iter().map(|e| e.usage_count as f32).collect();
        let n = counts.len() as f32;
        let mean = counts.iter().sum::<f32>() / n;
        if mean < 1e-10 { return 0.0; }
        let variance = counts.iter().map(|c| (c - mean).powi(2)).sum::<f32>() / n;
        self.config.balance_coeff * (variance / (mean * mean))
    }

    /// Reset les compteurs d'utilisation
    pub fn reset_usage(&mut self) {
        for expert in &mut self.experts {
            expert.usage_count = 0;
        }
    }

    /// Distribution d'utilisation des experts (normalisée)
    pub fn usage_distribution(&self) -> Vec<f32> {
        let total: u64 = self.experts.iter().map(|e| e.usage_count).sum();
        if total == 0 {
            vec![1.0 / self.experts.len() as f32; self.experts.len()]
        } else {
            self.experts.iter().map(|e| e.usage_count as f32 / total as f32).collect()
        }
    }
}

/// Sortie du MoE
#[derive(Debug, Clone)]
pub struct MoEOutput {
    /// Vecteur de sortie combiné
    pub output: Vec<f32>,
    /// Sorties individuelles : (expert_idx, poids, sortie)
    pub expert_outputs: Vec<(usize, f32, Vec<f32>)>,
    /// Loss de balance (à ajouter à la loss totale)
    pub balance_loss: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_moe_forward() {
        let config = MoEConfig {
            input_dim: 8,
            output_dim: 4,
            n_experts: 4,
            top_k: 2,
            balance_coeff: 0.01,
        };
        let mut moe = MixtureOfExperts::new(config);
        let x = vec![1.0, 0.5, -0.3, 0.2, 0.0, 0.1, -0.5, 0.8];
        let out = moe.forward(&x);
        assert_eq!(out.output.len(), 4);
        assert_eq!(out.expert_outputs.len(), 2); // top-2
    }

    #[test]
    fn test_top_k_sums_to_one() {
        let gating = GatingNetwork::new(8, 6);
        let x = vec![1.0; 8];
        let gates = gating.top_k_gate(&x, 3, false);
        let sum: f32 = gates.iter().map(|(_, w)| w).sum();
        assert!((sum - 1.0).abs() < 1e-4, "Top-k weights should sum to 1, got {}", sum);
    }
}
