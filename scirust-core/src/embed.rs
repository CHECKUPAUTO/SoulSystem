//! Text embedding engine using random-projection locality-sensitive hashing.
//!
//! Produces fixed-size embeddings (default 128-d) suitable for semantic
//! similarity via cosine distance. Uses seed-based hashing for deterministic
//! but pseudo-random projections — no model loading required.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Lightweight text embedding engine with random-projection LSH.
pub struct EmbeddingEngine {
    dim: usize,
    /// Fixed projection matrix: [dim × hash_space] (pseudo-random, seeded).
    proj: Vec<f32>,
    hash_space: usize,
}

impl EmbeddingEngine {
    /// Create a new embedding engine with default dimension 128.
    pub fn new(_vocab: &[&str]) -> Self {
        Self::with_dim(128)
    }

    /// Create with a specific output dimension.
    pub fn with_dim(dim: usize) -> Self {
        let hash_space = 1024; // internal feature space
        let proj = Self::generate_projection(dim, hash_space);
        Self {
            dim,
            proj,
            hash_space,
        }
    }

    /// Embedding dimension.
    #[inline]
    pub fn dim(&self) -> usize {
        self.dim
    }

    /// Generate an embedding vector for the given text.
    pub fn embed(&self, text: &str) -> Vec<f32> {
        let hash_vec = Self::text_to_hash_vec(text, self.hash_space);
        let mut out = vec![0.0f32; self.dim];
        for (i, o) in out.iter_mut().enumerate() {
            let mut acc = 0.0f32;
            for (j, &h) in hash_vec.iter().enumerate() {
                acc += h as f32 * self.proj[i * self.hash_space + j];
            }
            *o = acc / (self.hash_space as f32).sqrt();
        }
        // L2 normalize
        let norm: f32 = out.iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in &mut out {
                *v /= norm;
            }
        }
        out
    }

    /// Generate deterministic pseudo-random projection matrix.
    fn generate_projection(dim: usize, hash_space: usize) -> Vec<f32> {
        let mut proj = vec![0.0f32; dim * hash_space];
        let mut seed = 42u64;
        for v in proj.iter_mut() {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            *v = if (seed >> 32) as i32 >= 0 { 1.0 } else { -1.0 } / (dim as f32).sqrt();
        }
        proj
    }

    /// Hash text into a fixed-size binary feature vector using n-gram hashing.
    fn text_to_hash_vec(text: &str, size: usize) -> Vec<i8> {
        let mut vec = vec![0i8; size];
        let lower = text.to_lowercase();
        let bytes = lower.as_bytes();

        // Character trigram hashing
        for i in 0..bytes.len().saturating_sub(2) {
            let mut h = DefaultHasher::new();
            bytes[i..i + 3].hash(&mut h);
            let idx = (h.finish() as usize) % size;
            vec[idx] = 1i8;
        }

        // Full-text hash (captures overall structure)
        if !text.is_empty() {
            let mut h = DefaultHasher::new();
            text.hash(&mut h);
            let idx = (h.finish() as usize) % size;
            vec[idx] = 1i8;
        }

        vec
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embed_produces_normalized() {
        let e = EmbeddingEngine::new(&[]);
        let v = e.embed("hello world");
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5);
    }

    #[test]
    fn same_text_same_embedding() {
        let e = EmbeddingEngine::new(&[]);
        let a = e.embed("test");
        let b = e.embed("test");
        assert_eq!(a, b);
    }

    #[test]
    fn different_texts_different() {
        let e = EmbeddingEngine::new(&[]);
        let a = e.embed("the cat sat on the mat");
        let b = e.embed("quantum chromodynamics");
        let sim: f32 = a.iter().zip(&b).map(|(x, y)| x * y).sum();
        assert!(sim < 0.9); // should not be identical
    }

    #[test]
    fn similar_texts_are_closer() {
        let e = EmbeddingEngine::new(&[]);
        let a = e.embed("the cat sat on the mat");
        let b = e.embed("the cat sat on the rug");
        let c = e.embed("quantum field theory in curved spacetime");
        let sim_ab: f32 = a.iter().zip(&b).map(|(x, y)| x * y).sum();
        let sim_ac: f32 = a.iter().zip(&c).map(|(x, y)| x * y).sum();
        assert!(
            sim_ab > sim_ac,
            "similar texts should have higher cosine similarity"
        );
    }
}
