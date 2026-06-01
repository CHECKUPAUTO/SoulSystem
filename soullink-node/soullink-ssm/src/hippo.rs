//! HiPPO (High-order Polynomial Projection Operators) initialization.
//!
//! Provides the HiPPO-LegS matrix used to initialize the SSM state
//! transition matrix A, enabling long-range memory via optimal polynomial
//! projections. References:
//!   - HiPPO: Recurrent Memory with Optimal Polynomial Projections (Gu et al., 2020)
//!   - HiPPO-LegS: Scale-invariant Legendre variant

use ndarray::{Array1, Array2};

/// HiPPO initializer for SSM transition matrices.
pub struct HiPPOInitializer;

impl HiPPOInitializer {
    /// Generate the HiPPO-LegS (Legendre Scale-invariant) matrix A.
    ///
    /// Produces an (N, N) matrix where:
    ///   A_{nk} = -((2n+1)^{1/2} (2k+1)^{1/2})  if n > k
    ///   A_{nk} = -(n+1)                          if n == k
    ///   A_{nk} = 0                                if n < k
    ///
    /// This gives a lower-triangular structure with favorable conditioning.
    ///
    /// # Arguments
    ///
    /// * `n` - State dimension
    ///
    /// # Returns
    ///
    /// An (n, n) HiPPO-LegS matrix.
    pub fn legs(n: usize) -> Array2<f64> {
        let mut a = Array2::zeros((n, n));

        for i in 0..n {
            for j in 0..=i {
                let val = if i == j {
                    -(i as f64 + 1.0)
                } else {
                    let ni = 2.0 * i as f64 + 1.0;
                    let nj = 2.0 * j as f64 + 1.0;
                    -ni.sqrt() * nj.sqrt()
                };
                a[[i, j]] = val;
            }
        }

        a
    }

    /// Generate the diagonal HiPPO matrix used in S4D.
    ///
    /// The diagonal approximation: A_{kk} = -0.5 + i * π * k
    /// for k = 0, ..., N-1 (normalized frequencies).
    ///
    /// # Arguments
    ///
    /// * `n` - State dimension
    ///
    /// # Returns
    ///
    /// Complex-valued diagonal as a real (n, 2) matrix [real, imag].
    pub fn s4d_diagonal(n: usize) -> Array2<f64> {
        let mut diag = Array2::zeros((n, 2));

        for k in 0..n {
            diag[[k, 0]] = -0.5; // real part
            diag[[k, 1]] = std::f64::consts::PI * k as f64; // imag part
        }

        diag
    }

    /// Compute the HiPPO-Normal matrix for SSM (simplified normal form).
    ///
    /// This is used by S4's normal + low-rank factorization:
    ///   A = A_normal - P P^T
    ///
    /// Returns (A_normal, P) where A_normal is diagonal and P is low-rank.
    ///
    /// # Arguments
    ///
    /// * `n` - State dimension
    ///
    /// # Returns
    ///
    /// `(A_normal_diag, P_matrix)` — A_normal_diag shape (n,), P shape (n, n)
    pub fn normal_plus_low_rank(n: usize) -> (Array1<f64>, Array2<f64>) {
        // Diagonal part: A_{kk} = -1/2
        let a_normal = Array1::from_elem(n, -0.5);

        // Low-rank factor P: P_{ik} = sqrt((2i+1)(2k+1))
        let mut p = Array2::zeros((n, n));
        for i in 0..n {
            for k in 0..n {
                p[[i, k]] = ((2.0 * i as f64 + 1.0) * (2.0 * k as f64 + 1.0)).sqrt();
            }
        }

        (a_normal, p)
    }

    /// Generate a structured HiPPO matrix for use in convolutional kernels.
    ///
    /// Returns the full (N, N) generalized HiPPO matrix with Vandermonde structure.
    ///
    /// # Arguments
    ///
    /// * `n` - State dimension
    /// * `t_max` - Maximum time horizon (default: 1.0)
    ///
    /// # Returns
    ///
    /// (n, n) HiPPO matrix
    pub fn generalized(n: usize, t_max: f64) -> Array2<f64> {
        let mut a = Array2::zeros((n, n));

        for i in 0..n {
            for j in 0..=i {
                let val = if i == j {
                    -(i as f64 + 1.0) / t_max
                } else {
                    -((2.0 * i as f64 + 1.0) * (2.0 * j as f64 + 1.0)).sqrt() / t_max
                };
                a[[i, j]] = val;
            }
        }
        // HiPPO-LegS is lower-triangular: upper triangle (j > i) stays zero
        a
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_legs_lower_triangular() {
        let n = 4;
        let a = HiPPOInitializer::legs(n);

        // Upper triangle (excluding diagonal) should be zero
        for i in 0..n {
            for j in (i + 1)..n {
                assert!(
                    a[[i, j]].abs() < 1e-10,
                    "A[{}][{}] = {}, expected 0",
                    i,
                    j,
                    a[[i, j]]
                );
            }
        }

        // Diagonal should be negative
        for i in 0..n {
            assert!(
                a[[i, i]] < 0.0,
                "Diagonal A[{}][{}] should be negative",
                i,
                i
            );
        }
    }

    #[test]
    fn test_s4d_diagonal() {
        let n = 8;
        let d = HiPPOInitializer::s4d_diagonal(n);

        assert_eq!(d.shape(), &[n, 2]);

        // Real parts should all be -0.5
        for k in 0..n {
            assert!((d[[k, 0]] - (-0.5)).abs() < 1e-10);
        }

        // Imaginary parts should increase linearly
        assert!((d[[1, 1]] - std::f64::consts::PI).abs() < 1e-10);
    }

    #[test]
    fn test_normal_plus_low_rank() {
        let n = 4;
        let (a_norm, p) = HiPPOInitializer::normal_plus_low_rank(n);

        assert_eq!(a_norm.len(), n);
        assert_eq!(p.shape(), &[n, n]);

        // A_normal should be -0.5 everywhere
        for i in 0..n {
            assert!((a_norm[i] + 0.5).abs() < 1e-10);
        }

        // P should be symmetric
        for i in 0..n {
            for j in 0..n {
                assert!((p[[i, j]] - p[[j, i]]).abs() < 1e-10);
            }
        }
    }

    #[test]
    fn test_generalized_shape() {
        let a = HiPPOInitializer::generalized(5, 1.0);
        assert_eq!(a.shape(), &[5, 5]);
    }
}
