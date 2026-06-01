//! # Softmax Attention : attn = softmax(QK^T / √d) V
//!
//! Pour le Social Organ — alignement des vecteurs latents entre organes.

use serde::{Deserialize, Serialize};

/// Résultat d'une couche d'attention
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttentionOutput {
    /// Vecteurs de sortie (seq_len × d_v)
    pub output: Vec<Vec<f32>>,
    /// Poids d'attention (seq_len × seq_len)
    pub weights: Vec<Vec<f32>>,
}

/// Softmax sur un vecteur
pub fn softmax(logits: &[f32]) -> Vec<f32> {
    let max = logits.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let exps: Vec<f32> = logits.iter().map(|x| (x - max).exp()).collect();
    let sum: f32 = exps.iter().sum();
    if sum < 1e-12 {
        vec![1.0 / logits.len() as f32; logits.len()]
    } else {
        exps.iter().map(|e| e / sum).collect()
    }
}

/// Scaled Dot-Product Attention
///
/// Q: (seq_q × d_k) queries
/// K: (seq_k × d_k) keys
/// V: (seq_k × d_v) values
///
/// attn = softmax(Q K^T / √d_k) V
pub fn scaled_dot_product_attention(
    queries: &[Vec<f32>],
    keys: &[Vec<f32>],
    values: &[Vec<f32>],
) -> AttentionOutput {
    let seq_q = queries.len();
    let seq_k = keys.len();
    let d_k = if queries.is_empty() {
        1
    } else {
        queries[0].len()
    };
    let d_v = if values.is_empty() {
        1
    } else {
        values[0].len()
    };
    let scale = (d_k as f32).sqrt();

    // Calcul des scores : Q K^T / √d_k
    let mut weights = vec![vec![0.0; seq_k]; seq_q];
    for i in 0..seq_q {
        let mut scores = vec![0.0; seq_k];
        for j in 0..seq_k {
            let dot: f32 = queries[i]
                .iter()
                .zip(keys[j].iter())
                .map(|(q, k)| q * k)
                .sum();
            scores[j] = dot / scale;
        }
        // Softmax
        weights[i] = softmax(&scores);
    }

    // Output : weights × V
    let mut output = vec![vec![0.0; d_v]; seq_q];
    for i in 0..seq_q {
        for j in 0..seq_k {
            let w = weights[i][j];
            for d in 0..d_v {
                output[i][d] += w * values[j][d];
            }
        }
    }

    AttentionOutput { output, weights }
}

/// Multi-Head Attention
///
/// Projette Q, K, V dans `n_heads` sous-espaces, applique l'attention,
/// et concatène les résultats.
pub struct MultiHeadAttention {
    pub n_heads: usize,
    pub d_model: usize,
    pub d_k: usize,
    pub d_v: usize,
    /// Poids de projection (n_heads × d_model × d_k) pour Q, K, V
    pub w_q: Vec<Vec<Vec<f32>>>,
    pub w_k: Vec<Vec<Vec<f32>>>,
    pub w_v: Vec<Vec<Vec<f32>>>,
    /// Projection de sortie (n_heads * d_v × d_model)
    pub w_o: Vec<Vec<f32>>,
}

impl MultiHeadAttention {
    /// Créer avec des poids initialisés aléatoirement (Xavier)
    pub fn new(d_model: usize, n_heads: usize) -> Self {
        let d_k = d_model / n_heads;
        let d_v = d_k;
        let scale = (2.0 / (d_model + d_k) as f32).sqrt();

        let init_weights = |rows: usize, cols: usize| -> Vec<Vec<f32>> {
            let mut rng = rand::thread_rng();
            (0..rows)
                .map(|_| {
                    (0..cols)
                        .map(|_| (rng.gen::<f32>() * 2.0 - 1.0) * scale)
                        .collect()
                })
                .collect()
        };

        let w_q: Vec<Vec<Vec<f32>>> = (0..n_heads).map(|_| init_weights(d_model, d_k)).collect();
        let w_k: Vec<Vec<Vec<f32>>> = (0..n_heads).map(|_| init_weights(d_model, d_k)).collect();
        let w_v: Vec<Vec<Vec<f32>>> = (0..n_heads).map(|_| init_weights(d_model, d_v)).collect();
        let w_o = init_weights(n_heads * d_v, d_model);

        Self {
            n_heads,
            d_model,
            d_k,
            d_v,
            w_q,
            w_k,
            w_v,
            w_o,
        }
    }

    /// Forward pass
    pub fn forward(&self, input: &[Vec<f32>]) -> AttentionOutput {
        let seq_len = input.len();
        let mut all_head_outputs: Vec<Vec<Vec<f32>>> = Vec::new();
        let mut avg_weights = vec![vec![0.0; seq_len]; seq_len];

        for h in 0..self.n_heads {
            // Projeter Q, K, V
            let q_proj = project(input, &self.w_q[h]);
            let k_proj = project(input, &self.w_k[h]);
            let v_proj = project(input, &self.w_v[h]);

            let attn = scaled_dot_product_attention(&q_proj, &k_proj, &v_proj);

            // Accumuler les poids pour visualisation
            for i in 0..seq_len {
                for j in 0..seq_len {
                    avg_weights[i][j] += attn.weights[i][j] / self.n_heads as f32;
                }
            }

            all_head_outputs.push(attn.output);
        }

        // Concaténer les heads
        let mut concat = vec![vec![0.0; self.n_heads * self.d_v]; seq_len];
        for i in 0..seq_len {
            for h in 0..self.n_heads {
                for d in 0..self.d_v {
                    concat[i][h * self.d_v + d] = all_head_outputs[h][i][d];
                }
            }
        }

        // Projection de sortie
        let output = project(&concat, &self.w_o);

        AttentionOutput {
            output,
            weights: avg_weights,
        }
    }
}

/// Projection linéaire : X @ W (chaque vecteur de X multiplié par W)
fn project(input: &[Vec<f32>], weights: &[Vec<f32>]) -> Vec<Vec<f32>> {
    let rows = weights.len();
    let cols = if weights.is_empty() {
        0
    } else {
        weights[0].len()
    };

    input
        .iter()
        .map(|x| {
            let mut out = vec![0.0; cols];
            for j in 0..cols {
                for i in 0..rows.min(x.len()) {
                    out[j] += x[i] * weights[i][j];
                }
            }
            out
        })
        .collect()
}

use rand::Rng;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_softmax_sums_to_one() {
        let logits = vec![1.0, 2.0, 3.0, 4.0];
        let probs = softmax(&logits);
        let sum: f32 = probs.iter().sum();
        assert!((sum - 1.0).abs() < 1e-5);
        // Dernier élément devrait être le plus grand
        assert!(probs[3] > probs[0]);
    }

    #[test]
    fn test_self_attention() {
        let seq = vec![
            vec![1.0, 0.0, 0.0, 0.0],
            vec![0.0, 1.0, 0.0, 0.0],
            vec![0.0, 0.0, 1.0, 0.0],
        ];
        let attn = scaled_dot_product_attention(&seq, &seq, &seq);
        assert_eq!(attn.output.len(), 3);
        assert_eq!(attn.weights.len(), 3);
        // Poids d'attention doivent sommer à 1
        let sum: f32 = attn.weights[0].iter().sum();
        assert!((sum - 1.0).abs() < 1e-5);
    }
}
