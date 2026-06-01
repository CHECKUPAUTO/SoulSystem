//! # Contrastive Loss (InfoNCE)
//!
//! L = -log( exp(sim(q⁺, q⁻)) / Σ_k exp(sim(q, q_k)) )
//!
//! Aligne les représentations de concepts entre organes.

use serde::{Deserialize, Serialize};

/// Configuration de la loss contrastive
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContrastiveConfig {
    /// Température τ (contrôle la dureté du softmax)
    pub temperature: f32,
    /// Dimension de la projection
    pub projection_dim: usize,
}

impl Default for ContrastiveConfig {
    fn default() -> Self {
        Self {
            temperature: 0.07,
            projection_dim: 128,
        }
    }
}

/// Similarité cosinus entre deux vecteurs
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len());
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a < 1e-10 || norm_b < 1e-10 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}

/// InfoNCE Loss pour une paire positive (anchor, positive) contre des négatifs
///
/// L = -log( exp(sim(a, p)/τ) / (exp(sim(a, p)/τ) + Σ_k exp(sim(a, n_k)/τ)) )
pub fn info_nce_loss(
    anchor: &[f32],
    positive: &[f32],
    negatives: &[Vec<f32>],
    temperature: f32,
) -> f32 {
    let sim_pos = cosine_similarity(anchor, positive) / temperature;

    let mut log_sum_exp_parts: Vec<f32> = vec![sim_pos];
    for neg in negatives {
        let sim_neg = cosine_similarity(anchor, neg) / temperature;
        log_sum_exp_parts.push(sim_neg);
    }

    // log-sum-exp stable
    let max = log_sum_exp_parts
        .iter()
        .cloned()
        .fold(f32::NEG_INFINITY, f32::max);
    let log_sum_exp = max
        + log_sum_exp_parts
            .iter()
            .map(|x| (x - max).exp())
            .sum::<f32>()
            .ln();

    // L = -sim_pos + log_sum_exp
    -sim_pos + log_sum_exp
}

/// NT-Xent (Normalized Temperature-scaled Cross Entropy)
/// Variante symétrique pour un batch de paires
///
/// Chaque paire (z_i, z_j) est positive, tout le reste est négatif.
/// L = (1/2N) Σ_k [l(k, k+N) + l(k+N, k)]
pub fn nt_xent_loss(embeddings: &[Vec<f32>], temperature: f32) -> f32 {
    let n = embeddings.len();
    if n < 2 {
        return 0.0;
    }

    // Assumer que les paires positives sont (i, i + n/2) pour i < n/2
    let half = n / 2;
    let mut total_loss = 0.0f32;

    for i in 0..half {
        let anchor = &embeddings[i];
        let positive = &embeddings[i + half];

        let sim_pos = cosine_similarity(anchor, positive) / temperature;

        // Tous les autres sont négatifs
        let mut log_denom_parts = vec![sim_pos];
        for j in 0..n {
            if j != i && j != i + half {
                let sim_neg = cosine_similarity(anchor, &embeddings[j]) / temperature;
                log_denom_parts.push(sim_neg);
            }
        }

        let max = log_denom_parts
            .iter()
            .cloned()
            .fold(f32::NEG_INFINITY, f32::max);
        let log_sum_exp = max
            + log_denom_parts
                .iter()
                .map(|x| (x - max).exp())
                .sum::<f32>()
                .ln();

        total_loss += -sim_pos + log_sum_exp;
    }

    total_loss / half as f32
}

/// Gradient de la InfoNCE loss par rapport à l'anchor
///
/// Pour mettre à jour les embeddings des organes.
pub fn info_nce_gradient(
    anchor: &[f32],
    positive: &[f32],
    negatives: &[Vec<f32>],
    temperature: f32,
) -> Vec<f32> {
    let dim = anchor.len();

    // Calculer les similarités et les softmax weights
    let sim_pos = cosine_similarity(anchor, positive) / temperature;
    let mut sims: Vec<f32> = vec![sim_pos];
    let mut all_vecs: Vec<&[f32]> = vec![positive];

    for neg in negatives {
        sims.push(cosine_similarity(anchor, neg) / temperature);
        all_vecs.push(neg);
    }

    // Softmax des similarités
    let max = sims.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let exps: Vec<f32> = sims.iter().map(|s| (s - max).exp()).collect();
    let sum_exp: f32 = exps.iter().sum();
    let weights: Vec<f32> = exps.iter().map(|e| e / sum_exp).collect();

    // Gradient ≈ (1/τ) (Σ_k w_k * (normalized k) - normalized positive)
    let norm_a: f32 = anchor.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-10);

    let mut grad = vec![0.0; dim];
    for (k, vec_k) in all_vecs.iter().enumerate() {
        let norm_k: f32 = vec_k.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-10);
        for d in 0..dim {
            grad[d] += weights[k] * vec_k[d] / (norm_a * norm_k);
        }
    }

    // Soustraire le positif
    let norm_p: f32 = positive
        .iter()
        .map(|x| x * x)
        .sum::<f32>()
        .sqrt()
        .max(1e-10);
    for d in 0..dim {
        grad[d] -= positive[d] / (norm_a * norm_p);
        grad[d] /= temperature;
    }

    grad
}

/// Projection MLP simple pour l'espace contrastif
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Projector {
    pub w1: Vec<Vec<f32>>,
    pub b1: Vec<f32>,
    pub w2: Vec<Vec<f32>>,
    pub b2: Vec<f32>,
}

impl Projector {
    pub fn new(input_dim: usize, hidden_dim: usize, output_dim: usize) -> Self {
        let init = |rows: usize, cols: usize| -> Vec<Vec<f32>> {
            let s = (2.0 / rows as f32).sqrt();
            let mut rng = rand::thread_rng();
            (0..rows)
                .map(|_| {
                    (0..cols)
                        .map(|_| (rng.gen::<f32>() * 2.0 - 1.0) * s)
                        .collect()
                })
                .collect()
        };

        Self {
            w1: init(input_dim, hidden_dim),
            b1: vec![0.0; hidden_dim],
            w2: init(hidden_dim, output_dim),
            b2: vec![0.0; output_dim],
        }
    }

    /// Forward : ReLU(x W1 + b1) W2 + b2, puis L2 normalisation
    pub fn project(&self, x: &[f32]) -> Vec<f32> {
        let hidden_dim = self.b1.len();
        let output_dim = self.b2.len();

        // Hidden = ReLU(x @ W1 + b1)
        let mut hidden = self.b1.clone();
        for j in 0..hidden_dim {
            for i in 0..x.len().min(self.w1.len()) {
                hidden[j] += x[i] * self.w1[i][j];
            }
            hidden[j] = hidden[j].max(0.0); // ReLU
        }

        // Output = hidden @ W2 + b2
        let mut out = self.b2.clone();
        for j in 0..output_dim {
            for i in 0..hidden_dim {
                out[j] += hidden[i] * self.w2[i][j];
            }
        }

        // L2 normalisation
        let norm: f32 = out.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-10);
        out.iter_mut().for_each(|x| *x /= norm);
        out
    }
}

use rand::Rng;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_info_nce_positive_lower() {
        let anchor = vec![1.0, 0.0, 0.0];
        let positive = vec![0.9, 0.1, 0.0]; // similaire
        let negatives = vec![
            vec![0.0, 1.0, 0.0],
            vec![0.0, 0.0, 1.0],
            vec![-1.0, 0.0, 0.0], // opposé
        ];

        let loss = info_nce_loss(&anchor, &positive, &negatives, 0.1);
        assert!(loss > 0.0, "InfoNCE loss devrait être > 0");

        // Avec un positif parfait, la loss devrait être plus basse
        let loss_perfect = info_nce_loss(&anchor, &anchor, &negatives, 0.1);
        assert!(loss_perfect < loss);
    }
}
