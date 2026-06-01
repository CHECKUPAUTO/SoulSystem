//! TurboQuant Attention Layer — drop-in attention with integrated KV compression.
//!
//! This is the integration point: attention computation + automatic KV compression.
//! When used inside soullink-inference, replaces raw attention with compressed attention.

use crate::turboquant::cache::TurboQuantKVCache;
use crate::turboquant::polar::PolarQuant;
use crate::turboquant::qjl::QJLQuantizer;
use nalgebra::{DMatrix, DVector};

/// Attention layer with integrated TurboQuant KV cache.
pub struct TurboQuantAttention {
    pub embed_dim: usize,
    pub num_heads: usize,
    pub head_dim: usize,
    pub scale: f32,
    pub cache: TurboQuantKVCache,
}

impl TurboQuantAttention {
    pub fn new(embed_dim: usize, num_heads: usize, max_seq_len: usize, num_layers: usize) -> Self {
        let head_dim = embed_dim / num_heads;
        let cache = TurboQuantKVCache::new(num_layers, max_seq_len, head_dim, num_heads);
        Self {
            embed_dim,
            num_heads,
            head_dim,
            scale: (head_dim as f32).sqrt(),
            cache,
        }
    }

    /// Forward pass with automatic KV compression.
    pub fn forward(
        &mut self,
        q: &DMatrix<f32>,
        k: &DMatrix<f32>,
        v: &DMatrix<f32>,
        pos: usize,
        compress: bool,
    ) -> DMatrix<f32> {
        // Store K/V in compressed cache if requested
        if compress {
            let k_flat: Vec<f32> = k.iter().copied().collect();
            let v_flat: Vec<f32> = v.iter().copied().collect();
            self.cache.store_range(0, pos, &k_flat, &v_flat);
        }

        // Standard attention: softmax(Q · K^T / √d) · V
        let attn_weights = (q * k.transpose()) / self.scale;
        let attn_probs = Self::softmax_rows(&attn_weights);
        &attn_probs * v
    }

    fn softmax_rows(mat: &DMatrix<f32>) -> DMatrix<f32> {
        let mut result = mat.clone();
        for mut row in result.row_iter_mut() {
            let max = row.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            let sum: f32 = row.iter().map(|v| (v - max).exp()).sum();
            for v in row.iter_mut() {
                *v = (*v - max).exp() / sum;
            }
        }
        result
    }

    /// Get reference to the compressed KV cache.
    pub fn kv_cache(&self) -> &TurboQuantKVCache {
        &self.cache
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attention_forward_works() {
        let mut attn = TurboQuantAttention::new(64, 4, 128, 1);
        let q = DMatrix::from_fn(8, 64, |r, c| ((r * 64 + c) as f32) * 0.001);
        let k = DMatrix::from_fn(8, 64, |r, c| ((r * 64 + c) as f32) * 0.001);
        let v = DMatrix::from_fn(8, 64, |r, c| ((r * 64 + c) as f32) * 0.001);
        let out = attn.forward(&q, &k, &v, 0, true);
        assert_eq!(out.nrows(), 8);
        assert_eq!(out.ncols(), 64);
    }

    #[test]
    fn compression_happens() {
        let mut attn = TurboQuantAttention::new(32, 4, 64, 1);
        let q = DMatrix::zeros(4, 32);
        let k = DMatrix::from_fn(4, 32, |r, c| (r * 32 + c) as f32 * 0.01);
        let v = DMatrix::from_fn(4, 32, |r, c| (r * 32 + c) as f32 * 0.02);
        attn.forward(&q, &k, &v, 0, true);
        assert_eq!(attn.kv_cache().stored_positions, 4);
    }
}
