//! Core SSM (State Space Model) kernel implementation.
//!
//! Implements the continuous-time linear state space model:
//!
//!   h'(t) = A h(t) + B x(t)    (state evolution)
//!   y(t)  = C h(t) + D x(t)    (output equation)
//!
//! With Zero-Order Hold (ZOH) discretization:
//!   Ā = exp(ΔA)
//!   B̄ = (ΔA)⁻¹(exp(ΔA) - I) · ΔB
//!
//! Supports both 1D sequence and multi-dimensional operations.

use ndarray::{Array1, Array2};
use std::f64;

/// Configuration for the SSM kernel.
#[derive(Debug, Clone)]
pub struct SSMConfig {
    /// State dimension (N)
    pub state_dim: usize,
    /// Input feature dimension (D)
    pub input_dim: usize,
    /// Discretization step (default: learned per input)
    pub delta: f64,
    /// Feedthrough / skip connection coefficient (D matrix)
    pub d_scale: f64,
    /// Whether to use complex-valued A matrix
    pub use_complex: bool,
}

impl Default for SSMConfig {
    fn default() -> Self {
        Self {
            state_dim: 16,
            input_dim: 1,
            delta: 0.01,
            d_scale: 0.0,
            use_complex: false,
        }
    }
}

/// Core SSM kernel handling state evolution and output projection.
///
/// This is the fundamental building block that S4 and S4D are built on.
/// Mamba extends this with input-dependent B, C, Δ (see `selective` module).
pub struct SSMKernel {
    /// State transition matrix (N, N) — typically HiPPO-initialized
    pub a: Array2<f64>,
    /// Input matrix (N, D)
    pub b: Array2<f64>,
    /// Output matrix (D, N)
    pub c: Array2<f64>,
    /// Feedthrough matrix (D, D) — typically 0 or scalar
    pub d: Array2<f64>,
    /// Configuration
    pub config: SSMConfig,
}

impl SSMKernel {
    /// Create a new SSM kernel with given matrices.
    pub fn new(a: Array2<f64>, b: Array2<f64>, c: Array2<f64>, config: SSMConfig) -> Self {
        let d = Array2::eye(config.input_dim) * config.d_scale;
        Self { a, b, c, d, config }
    }

    /// Discretize the continuous-time SSM using Zero-Order Hold (ZOH).
    ///
    /// Given step size Δ:
    ///   Ā = exp(ΔA)
    ///   B̄ = (ΔA)⁻¹(exp(ΔA) - I) · ΔB
    ///
    /// Uses scaling and squaring for matrix exponential approximation.
    ///
    /// # Arguments
    ///
    /// * `delta` - Step size (typically learned)
    ///
    /// # Returns
    ///
    /// `(A_discrete, B_discrete)` — the discretized matrices
    pub fn discretize(&self, delta: f64) -> (Array2<f64>, Array2<f64>) {
        let _a_bar = Self::matrix_exp(&self.a.mapv(|x| x * delta));
        // B̄ = A⁻¹(exp(ΔA) - I) · ΔB
        // Approximate using first-order Taylor for small Δ:
        // B̄ ≈ (I + ΔA/2 + (ΔA)²/6 + ...) · ΔB  ≈ ΔB  (first-order)
        // For numerical stability, use the exact form via the augmented matrix approach
        let n = self.config.state_dim;
        let d = self.config.input_dim;

        // Build augmented matrix: [ΔA, ΔB; 0, 0]
        let mut augmented = Array2::zeros((n + d, n + d));
        for i in 0..n {
            for j in 0..n {
                augmented[[i, j]] = self.a[[i, j]] * delta;
            }
        }
        for i in 0..n {
            for j in 0..d {
                augmented[[i, n + j]] = self.b[[i, j]] * delta;
            }
        }

        let exp_aug = Self::matrix_exp(&augmented);

        // Extract Ā = exp_aug[0:n, 0:n]
        let mut a_bar = Array2::zeros((n, n));
        for i in 0..n {
            for j in 0..n {
                a_bar[[i, j]] = exp_aug[[i, j]];
            }
        }

        // Extract B̄ = exp_aug[0:n, n:n+d]
        let mut b_bar = Array2::zeros((n, d));
        for i in 0..n {
            for j in 0..d {
                b_bar[[i, j]] = exp_aug[[i, n + j]];
            }
        }

        (a_bar, b_bar)
    }

    /// Compute matrix exponential using scaling and squaring.
    ///
    /// Delegates to the free function [`matrix_exp`].
    pub fn matrix_exp(m: &Array2<f64>) -> Array2<f64> {
        matrix_exp(m)
    }

    /// Compute the SSM convolution kernel for efficient training.
    ///
    /// For a sequence of length L, the convolution kernel K is:
    ///   K[i] = C · Āⁱ · B̄
    ///
    /// The output is then y = K ∗ x (convolution).
    ///
    /// # Arguments
    ///
    /// * `length` - Sequence length
    /// * `delta` - Step size for discretization
    ///
    /// # Returns
    ///
    /// Convolution kernel of shape (L,) for scalar input, or (D, L) for multi-dim
    pub fn convolution_kernel(&self, length: usize, delta: f64) -> Array2<f64> {
        let (a_bar, b_bar) = self.discretize(delta);
        let n = self.config.state_dim;
        let d = self.config.input_dim;

        let mut kernel = Array2::zeros((d, length));

        // Compute powers of Ā and multiply
        let mut a_power = Array2::eye(n);
        for t in 0..length {
            // K[t] = C · Ā^t · B̄
            let step = self.c.dot(&a_power).dot(&b_bar);
            for i in 0..d {
                kernel[[i, t]] = step[[i, 0]]; // assuming 1D input per channel
            }
            a_power = a_power.dot(&a_bar);
        }

        kernel
    }

    /// Scan (recurrent forward pass) over a sequence.
    ///
    /// State update:
    ///   h_t = Ā · h_{t-1} + B̄ · x_t
    ///   y_t = C · h_t + D · x_t
    ///
    /// # Arguments
    ///
    /// * `x` - Input sequence (L, D)
    /// * `delta` - Step size
    ///
    /// # Returns
    ///
    /// Output sequence (L, D)
    pub fn scan(&self, x: &Array2<f64>, delta: f64) -> Array2<f64> {
        let (a_bar, b_bar) = self.discretize(delta);
        let length = x.shape()[0];
        let d = self.config.input_dim;
        let n = self.config.state_dim;

        let mut h = Array1::zeros(n);
        let mut y = Array2::zeros((length, d));

        for t in 0..length {
            let xt = x.row(t).to_owned();
            // h_t = Ā · h_{t-1} + B̄ · x_t
            h = a_bar.dot(&h) + b_bar.dot(&xt);
            // y_t = C · h_t + D · x_t
            let yt = self.c.dot(&h) + self.d.dot(&xt);
            y.row_mut(t).assign(&yt);
        }

        y
    }
}

/// Compute matrix exponential using scaling and squaring.
///
/// For a matrix M, computes exp(M) using Taylor order 12 with
/// scaling & squaring (up to 2^10). Reference implementation shared
/// by SSMKernel, SelectiveSSM, and MambaBlock.
pub fn matrix_exp(m: &Array2<f64>) -> Array2<f64> {
    let n = m.shape()[0];
    assert_eq!(m.shape()[1], n, "Matrix must be square");

    let norm: f64 = m.iter().map(|x| x.abs()).fold(0.0, |a, b| a.max(b));

    let s = if norm > 1.0 {
        (norm.log2().ceil() as u32).min(10)
    } else {
        0
    };
    let scale = 2u32.pow(s) as f64;
    let m_scaled = m.mapv(|x| x / scale);

    let mut result = Array2::eye(n);
    let mut term = Array2::eye(n);
    for k in 1..=12 {
        term = term.dot(&m_scaled).mapv(|x| x / k as f64);
        result = result + &term;
    }

    let mut final_result = result;
    for _ in 0..s {
        final_result = final_result.dot(&final_result);
    }

    final_result
}

/// Discretize continuous-time SSM matrices using Zero-Order Hold (ZOH).
///
/// Builds the augmented matrix `[ΔA, Δb; 0, 0]` of shape (N+1, N+1),
/// computes its matrix exponential, and extracts:
///   - Ā = exp_aug[0:N, 0:N]
///   - b̄ = exp_aug[0:N, N]
///
/// This gives `h_t = Ā·h_{t-1} + b̄` where b̄ already incorporates
/// the input projection B(x)·x_t.
///
/// # Arguments
///
/// * `a` - State transition matrix (N, N)
/// * `b_vec` - Input column vector (N,) — already projected by B(x)
/// * `delta` - Step size Δ
///
/// # Returns
///
/// `(A_bar, B_bar)` — discretized matrices of shape (N, N) and (N,)
pub fn discretize_zoh(
    a: &Array2<f64>,
    b_vec: &Array1<f64>,
    delta: f64,
) -> (Array2<f64>, Array1<f64>) {
    let n = a.shape()[0];
    let mut augmented = Array2::zeros((n + 1, n + 1));

    for i in 0..n {
        for j in 0..n {
            augmented[[i, j]] = a[[i, j]] * delta;
        }
        augmented[[i, n]] = b_vec[i] * delta;
    }

    let exp_aug = matrix_exp(&augmented);

    let mut a_bar = Array2::zeros((n, n));
    let mut b_bar = Array1::zeros(n);
    for i in 0..n {
        for j in 0..n {
            a_bar[[i, j]] = exp_aug[[i, j]];
        }
        b_bar[i] = exp_aug[[i, n]];
    }

    (a_bar, b_bar)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matrix_exp_identity() {
        // exp(0) = I
        let zero: Array2<f64> = Array2::zeros((3, 3));
        let exp_zero = SSMKernel::matrix_exp(&zero);
        let eye: Array2<f64> = Array2::eye(3);

        for i in 0..3 {
            for j in 0..3 {
                assert!((exp_zero[[i, j]] - eye[[i, j]]).abs() < 1e-8);
            }
        }
    }

    #[test]
    fn test_matrix_exp_diagonal() {
        // exp(diag([1, 2])) = diag([e, e^2])
        let mut m = Array2::zeros((2, 2));
        m[[0, 0]] = 1.0;
        m[[1, 1]] = 2.0;

        let exp_m = SSMKernel::matrix_exp(&m);

        assert!((exp_m[[0, 0]] - std::f64::consts::E).abs() < 1e-6);
        assert!((exp_m[[1, 1]] - std::f64::consts::E.powi(2)).abs() < 1e-6);
    }

    #[test]
    fn test_ssm_scan_output_shape() {
        let config = SSMConfig {
            state_dim: 4,
            input_dim: 2,
            delta: 0.01,
            d_scale: 0.0,
            use_complex: false,
        };

        let a = Array2::eye(4) * -0.5;
        let b: Array2<f64> = Array2::zeros((4, 2));
        let c: Array2<f64> = Array2::zeros((2, 4));
        // Fill with small values
        let b = b.mapv(|_: f64| rand::random::<f64>() * 0.1);
        let c = c.mapv(|_: f64| rand::random::<f64>() * 0.1);

        let kernel = SSMKernel::new(a, b, c, config);
        let x: Array2<f64> = Array2::zeros((10, 2)).mapv(|_: f64| rand::random::<f64>() * 0.5);

        let y = kernel.scan(&x, 0.01);
        assert_eq!(y.shape(), &[10, 2]);
    }
}
