//! PolarQuant: Phase 1 of TurboQuant — Geometric Rotation
//!
//! Applies a random orthogonal rotation matrix R to KV vectors:
//!   y = x · R
//! This distributes information uniformly, making quantization trivial.
//! Random orthogonal matrices preserve L2 norm exactly.

use nalgebra::{DMatrix, DVector};
use rand::SeedableRng;
use rand_distr::{Distribution, StandardNormal};

/// PolarQuant rotation engine.
pub struct PolarQuant {
    dim: usize,
    /// Orthogonal rotation matrix R (dim × dim)
    r: DMatrix<f32>,
    /// Inverse rotation R^T (transpose for orthogonal matrices)
    r_t: DMatrix<f32>,
}

impl PolarQuant {
    /// Create with random orthogonal rotation via QR decomposition.
    pub fn new(dim: usize, seed: u64) -> Self {
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        let normal = StandardNormal;

        // Random Gaussian matrix H → QR → Q is orthogonal
        let h = DMatrix::from_fn(dim, dim, |_r, _c| normal.sample(&mut rng));
        let qr = h.qr();
        let q = qr.q();
        let q_t = q.transpose();

        Self {
            dim,
            r: q,
            r_t: q_t,
        }
    }

    pub fn dim(&self) -> usize {
        self.dim
    }

    /// Forward rotation: y = x · R  (x is row-major, each row is a dim-vector)
    pub fn rotate(&self, x: &DMatrix<f32>) -> DMatrix<f32> {
        x * &self.r
    }

    /// Inverse rotation: x ≈ y · R^T
    pub fn inverse_rotate(&self, y: &DMatrix<f32>) -> DMatrix<f32> {
        y * &self.r_t
    }

    /// Rotate a single vector (flat f32 slice of length dim).
    pub fn rotate_vec(&self, x: &[f32]) -> Vec<f32> {
        let col = DVector::from_row_slice(x);
        let rotated = &self.r * &col;
        rotated.iter().copied().collect()
    }

    /// Inverse rotate a single vector.
    pub fn inverse_rotate_vec(&self, y: &[f32]) -> Vec<f32> {
        let col = DVector::from_row_slice(y);
        let original = &self.r_t * &col;
        original.iter().copied().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_matrix() {
        let dim = 64;
        let pq = PolarQuant::new(dim, 42);
        let x = DMatrix::from_fn(4, dim, |r, c| (r * dim + c) as f32 * 0.01);
        let rotated = pq.rotate(&x);
        let recovered = pq.inverse_rotate(&rotated);
        let diff: f32 = (recovered - &x).iter().map(|v| v.abs()).sum::<f32>() / x.len() as f32;
        assert!(diff < 1e-4, "PolarQuant roundtrip error: {diff}");
    }

    #[test]
    fn roundtrip_vector() {
        let dim = 32;
        let pq = PolarQuant::new(dim, 7);
        let x: Vec<f32> = (0..dim).map(|i| i as f32 * 0.1).collect();
        let rotated = pq.rotate_vec(&x);
        let recovered = pq.inverse_rotate_vec(&rotated);
        let diff: f32 = recovered
            .iter()
            .zip(x.iter())
            .map(|(a, b)| (a - b).abs())
            .sum::<f32>()
            / dim as f32;
        assert!(diff < 1e-3, "Vec roundtrip error: {diff}");
    }

    #[test]
    fn preserves_norm() {
        let dim = 64;
        let pq = PolarQuant::new(dim, 99);
        let x = DMatrix::from_fn(1, dim, |_, c| c as f32 * 0.1);
        let x_norm: f32 = x.iter().map(|v| v * v).sum::<f32>().sqrt();
        let y = pq.rotate(&x);
        let y_norm: f32 = y.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!(
            (x_norm - y_norm).abs() < 1e-2,
            "Norm not preserved: {x_norm} vs {y_norm}"
        );
    }
}
