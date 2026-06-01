//! Selective State Space Model (S6 — Mamba's core innovation).
//!
//! The key insight of Mamba is that the SSM parameters Δ, B, C
//! should be *input-dependent* (selective), allowing the model to
//! modulate its state update based on the input content:
//!
//!   B(x)  = W_B · x        (input-dependent input projection)
//!   C(x)  = W_C · x        (input-dependent output projection)
//!   Δ(x)  = softplus(W_Δ · x)  (input-dependent step size)
//!
//! This enables content-aware reasoning while maintaining the
//! linear-time computational complexity of the scan.
//!
//! Also provides the multi-dimensional variant `MultiDimSelectiveSSM`
//! where each channel has its own N-dimensional state with per-channel Δ.

use ndarray::{Array1, Array2, ArrayView1};

use crate::parallel::MultiDimScan;
use crate::ssm;

/// Selective SSM with input-dependent B, C, Δ parameters.
///
/// The B and C matrices are computed per-timestep from the input,
/// giving the model the ability to "select" what to store in its
/// state and what to retrieve from it.
pub struct SelectiveSSM {
    /// State transition matrix (N, N) — not selective, HiPPO-initialized
    pub a: Array2<f64>,
    /// Learned projection for B (N, D) — base B matrix
    pub b_proj: Array2<f64>,
    /// Learned projection for C (N, D) — base C matrix
    pub c_proj: Array2<f64>,
    /// Learned projection for Δ (D,) — bias for step size
    pub delta_proj: Array1<f64>,
    /// Feedthrough / skip connection (scalar per channel)
    pub d: f64,
    /// State dimension
    pub state_dim: usize,
    /// Input / output dimension
    pub input_dim: usize,
    /// Discretization min/max bounds
    pub delta_min: f64,
    pub delta_max: f64,
}

impl SelectiveSSM {
    /// Create a new selective SSM layer.
    ///
    /// # Arguments
    ///
    /// * `state_dim` - State dimension (N)
    /// * `input_dim` - Input/output feature dimension (D)
    /// * `a` - HiPPO-initialized state matrix (N, N)
    pub fn new(state_dim: usize, input_dim: usize, a: Array2<f64>) -> Self {
        // Initialize projections with small random values
        let mut rng = rand::thread_rng();
        let b_proj: Array2<f64> = Array2::zeros((state_dim, input_dim)).mapv(|_: f64| {
            rand::Rng::gen_range(&mut rng, -0.1..0.1) * (1.0 / input_dim as f64).sqrt()
        });
        let c_proj: Array2<f64> = Array2::zeros((state_dim, input_dim)).mapv(|_: f64| {
            rand::Rng::gen_range(&mut rng, -0.1..0.1) * (1.0 / input_dim as f64).sqrt()
        });
        let delta_proj = Array1::from_elem(input_dim, 0.1);

        Self {
            a,
            b_proj,
            c_proj,
            delta_proj,
            d: 0.0,
            state_dim,
            input_dim,
            delta_min: 0.001,
            delta_max: 0.1,
        }
    }

    /// Compute input-dependent parameters for a single timestep.
    ///
    /// # Arguments
    ///
    /// * `x` - Input vector (D,)
    ///
    /// # Returns
    ///
    /// `(B_t, C_t, Δ_t)` — input-dependent matrices and step size
    #[allow(dead_code)]
    fn compute_params(&self, x: &ArrayView1<f64>) -> (Array2<f64>, Array2<f64>, f64) {
        // Δ = softplus(W_Δ · x)
        let raw_delta: f64 = self
            .delta_proj
            .iter()
            .zip(x.iter())
            .map(|(w, xi)| w * xi)
            .sum();
        // Clamp and apply softplus-like activation
        let delta = Self::softplus(raw_delta)
            .max(self.delta_min)
            .min(self.delta_max);

        // B(x) = W_B * x  → (N,)
        let b_vec: Array1<f64> = self.b_proj.dot(x);
        // C(x) = W_C * x  → (N,)
        let c_vec: Array1<f64> = self.c_proj.dot(x);

        // Reshape B and C to column/row matrices for matrix operations
        let b_mat = b_vec.into_shape_with_order((self.state_dim, 1)).unwrap();
        let c_mat = c_vec.into_shape_with_order((1, self.state_dim)).unwrap();

        (b_mat, c_mat, delta)
    }

    /// Softplus activation: log(1 + exp(x)).
    pub(crate) fn softplus(x: f64) -> f64 {
        if x > 20.0 {
            x // linear for large positive values
        } else if x < -20.0 {
            x.exp() // ≈ exp(x) for large negative
        } else {
            (1.0 + x.exp()).ln()
        }
    }

    /// Discretize the SSM for a given step size Δ.
    ///
    /// Delegates to [`ssm::discretize_zoh`].
    fn discretize(
        &self,
        a: &Array2<f64>,
        b: &Array2<f64>,
        delta: f64,
    ) -> (Array2<f64>, Array2<f64>) {
        let b_col = b.column(0).to_owned();
        let (a_bar, b_bar_1d) = ssm::discretize_zoh(a, &b_col, delta);
        let mut b_bar = Array2::zeros((self.state_dim, 1));
        for i in 0..self.state_dim {
            b_bar[[i, 0]] = b_bar_1d[i];
        }
        (a_bar, b_bar)
    }

    /// Forward pass over a sequence using the selective scan.
    ///
    /// Unlike the standard SSM, B, C, and Δ vary per timestep,
    /// which means the convolution kernel representation doesn't
    /// apply and we must use the recurrent scan.
    ///
    /// Delegates to `selective_scan` — see that method for details.
    ///
    /// # Arguments
    ///
    /// * `x` - Input sequence (L, D)
    ///
    /// # Returns
    ///
    /// Output sequence (L, D)
    pub fn forward(&self, x: &Array2<f64>) -> Array2<f64> {
        self.selective_scan(x)
    }

    /// Run selective scan and return only the final hidden state.
    ///
    /// Used for inference caching — captures `h_L` after processing
    /// the full sequence L.
    pub fn selective_scan_final_state(&self, x: &Array2<f64>) -> Array1<f64> {
        let length = x.shape()[0];
        let n = self.state_dim;
        let mut h = Array1::zeros(n);

        for t in 0..length {
            let xt = x.row(t);
            let b_vec: Array1<f64> = self.b_proj.dot(&xt);
            let raw_delta: f64 = self
                .delta_proj
                .iter()
                .zip(xt.iter())
                .map(|(w, xi)| w * xi)
                .sum();
            let delta = Self::softplus(raw_delta)
                .max(self.delta_min)
                .min(self.delta_max);

            let (a_bar, b_bar) = self.discretize(
                &self.a,
                &b_vec.clone().into_shape_with_order((n, 1)).unwrap(),
                delta,
            );
            let b_bar_1d = b_bar.column(0).to_owned();
            h = a_bar.dot(&h) + &b_bar_1d;
        }

        h
    }

    /// Correct selective scan implementation.
    ///
    /// The Mamba selective scan works as follows:
    /// 1. Project x → B(x), C(x), Δ(x) using learned matrices
    /// 2. Discretize: Ā(x) = exp(Δ(x)·A), B̄(x) = (∫exp(τA)dτ)·Δ(x)·B(x)
    /// 3. Recurrence: h_t = Ā_t · h_{t-1} + B̄_t    (B̄_t already includes x_t)
    /// 4. Output: y_t = C_t · h_t
    pub fn selective_scan(&self, x: &Array2<f64>) -> Array2<f64> {
        let length = x.shape()[0];
        let d = self.input_dim;
        let n = self.state_dim;

        let mut y = Array2::zeros((length, d));
        let mut h = Array1::zeros(n);

        for t in 0..length {
            let xt = x.row(t);

            // Input-dependent parameters
            // B(x) = W_B · x  ∈ R^N
            // C(x) = W_C · x  ∈ R^N
            let b_vec: Array1<f64> = self.b_proj.dot(&xt);
            let c_vec: Array1<f64> = self.c_proj.dot(&xt);

            // Δ(x) = softplus(W_Δ · x) ∈ R
            let raw_delta: f64 = self
                .delta_proj
                .iter()
                .zip(xt.iter())
                .map(|(w, xi)| w * xi)
                .sum();
            let delta = Self::softplus(raw_delta)
                .max(self.delta_min)
                .min(self.delta_max);

            // Discretize: compute Ā_t, B̄_t
            // B̄_t is (N,) — already incorporates x_t through B(x_t)
            let (a_bar, b_bar) = self.discretize(
                &self.a,
                &b_vec.clone().into_shape_with_order((n, 1)).unwrap(),
                delta,
            );
            let b_bar_1d = b_bar.column(0).to_owned();

            // State update: h_t = Ā_t · h_{t-1} + B̄_t
            h = a_bar.dot(&h) + &b_bar_1d;

            // Output: y_t = C_t · h_t + D · x_t
            let yt: f64 = c_vec.dot(&h) + self.d;
            y[[t, 0]] = yt;
            // For multi-dimensional, we'd add more output projections
            if d > 1 {
                // Copy across output dims (simplified — real Mamba uses
                // separate output projections per dimension)
                for j in 1..d {
                    y[[t, j]] = yt;
                }
            }
        }

        y
    }

    /// Parallel selective scan using Blelloch binary-tree reduction.
    ///
    /// Pre-computes all input-dependent parameters, runs the associative
    /// parallel scan from [`MultiDimScan::parallel_scan`], then computes
    /// outputs from the inclusive prefixes.
    ///
    /// When h₀ = 0 (default), h_t = B_combined[t], so the output
    /// y_t = C_t · B_combined[t] + D.
    ///
    /// # Arguments
    ///
    /// * `x` - Input sequence (L, D)
    ///
    /// # Returns
    ///
    /// Output sequence (L, D)
    pub fn selective_scan_parallel(&self, x: &Array2<f64>) -> Array2<f64> {
        let length = x.shape()[0];
        let d = self.input_dim;

        // Pre-compute all input-dependent parameters
        let mut a_matrices = Vec::with_capacity(length);
        let mut b_vectors = Vec::with_capacity(length);
        let mut c_vectors = Vec::with_capacity(length);

        for t in 0..length {
            let xt = x.row(t);

            let b_vec: Array1<f64> = self.b_proj.dot(&xt);
            let c_vec: Array1<f64> = self.c_proj.dot(&xt);

            let raw_delta: f64 = self
                .delta_proj
                .iter()
                .zip(xt.iter())
                .map(|(w, xi)| w * xi)
                .sum();
            let delta = Self::softplus(raw_delta)
                .max(self.delta_min)
                .min(self.delta_max);

            let (a_bar, b_bar) = ssm::discretize_zoh(&self.a, &b_vec, delta);
            a_matrices.push(a_bar);
            b_vectors.push(b_bar);
            c_vectors.push(c_vec);
        }

        // Parallel scan → inclusive prefixes: h_t = A_combined·h₀ + B_combined
        let prefixes = MultiDimScan::parallel_scan(&a_matrices, &b_vectors);

        // Compute outputs: h₀ = 0 so h_t = B_combined[t]
        let mut y = Array2::zeros((length, d));
        for t in 0..length {
            let h_t = &prefixes[t].1;
            let yt: f64 = c_vectors[t].dot(h_t) + self.d;
            y[[t, 0]] = yt;
            if d > 1 {
                for j in 1..d {
                    y[[t, j]] = yt;
                }
            }
        }

        y
    }
}

// ─── Multi-Dimensional Selective SSM ───────────────────────────────────────

/// Multi-dimensional Selective SSM.
///
/// Each of the `d_inner` channels has its own N-dimensional state, with
/// per-channel Δ step sizes and C output vectors. The A state transition
/// matrix and B projection are shared across channels.
///
/// For each timestep t:
///   1. B(x_t) = B_proj · x_t → (N,) shared across channels
///   2. For each channel i:
///      a. Δ_i = softplus(delta_weight[i,:] · x_t + delta_bias[i])
///      b. C_i = C_proj[i,:] → (N,) fixed per-channel output vector
///      c. Discretize: Ā_i = exp(Δ_i · A),  B̄_i via augmented matrix
///      d. h_i = Ā_i · h_i + B̄_i   (B̄_i already incorporates B(x_t))
///      e. y[t,i] = C_i · h_i + D · x[t,i]
pub struct MultiDimSelectiveSSM {
    /// State transition matrix (N, N) — shared, HiPPO-initialized
    pub a: Array2<f64>,
    /// B projection (N, d_inner) — B(x) = B_proj @ x → (N,) shared
    pub b_proj: Array2<f64>,
    /// C vectors (d_inner, N) — per-channel output projection
    pub c_proj: Array2<f64>,
    /// Δ weight (d_inner, d_inner) — per-channel step size weight
    pub delta_weight: Array2<f64>,
    /// Δ bias (d_inner,) — per-channel step size bias
    pub delta_bias: Array1<f64>,
    /// Feedthrough / skip connection
    pub d: f64,
    /// State dimension (N)
    pub state_dim: usize,
    /// Input/output dimension (d_inner)
    pub input_dim: usize,
    /// Discretization bounds
    pub delta_min: f64,
    pub delta_max: f64,
}

impl MultiDimSelectiveSSM {
    /// Create a new multi-dimensional selective SSM.
    pub fn new(state_dim: usize, input_dim: usize, a: Array2<f64>) -> Self {
        let mut rng = rand::thread_rng();
        let scale = (1.0 / input_dim as f64).sqrt();

        let b_proj: Array2<f64> = Array2::zeros((state_dim, input_dim))
            .mapv(|_: f64| rand::Rng::gen_range(&mut rng, -scale..scale));
        let c_proj: Array2<f64> = Array2::zeros((input_dim, state_dim))
            .mapv(|_: f64| rand::Rng::gen_range(&mut rng, -scale..scale));
        let delta_weight: Array2<f64> = Array2::zeros((input_dim, input_dim))
            .mapv(|_: f64| rand::Rng::gen_range(&mut rng, -0.01..0.01));
        let delta_bias = Array1::from_elem(input_dim, 0.1);

        Self {
            a,
            b_proj,
            c_proj,
            delta_weight,
            delta_bias,
            d: 0.0,
            state_dim,
            input_dim,
            delta_min: 0.001,
            delta_max: 0.1,
        }
    }

    /// Softplus activation: log(1 + exp(x)).
    pub(crate) fn softplus(x: f64) -> f64 {
        if x > 20.0 {
            x
        } else if x < -20.0 {
            x.exp()
        } else {
            (1.0 + x.exp()).ln()
        }
    }

    /// Discretize A and B with a given step size Δ.
    fn discretize(&self, b_vec: &Array1<f64>, delta: f64) -> (Array2<f64>, Array1<f64>) {
        ssm::discretize_zoh(&self.a, b_vec, delta)
    }

    /// Multi-dimensional selective scan over a sequence.
    ///
    /// Each of the d_inner channels maintains its own N-dimensional state.
    pub fn selective_scan(&self, x: &Array2<f64>) -> Array2<f64> {
        let length = x.shape()[0];
        let d_inner = self.input_dim;
        let n = self.state_dim;

        let mut y = Array2::zeros((length, d_inner));
        let mut h = Array2::zeros((d_inner, n));

        for t in 0..length {
            let xt = x.row(t);

            // Shared B(x) = B_proj @ x → (N,)
            let b_shared: Array1<f64> = self.b_proj.dot(&xt);

            for i in 0..d_inner {
                // Δ_i = softplus(delta_weight[i,:] · x + delta_bias[i])
                let raw_delta: f64 = self.delta_weight.row(i).dot(&xt) + self.delta_bias[i];
                let delta = Self::softplus(raw_delta)
                    .max(self.delta_min)
                    .min(self.delta_max);

                // Discretize with per-channel Δ
                let (a_bar, b_bar) = self.discretize(&b_shared, delta);

                // Per-channel state
                let hi_prev = h.row(i).to_owned();
                // h_i = Ā · h_i + B̄
                let hi_new = a_bar.dot(&hi_prev) + &b_bar;
                h.row_mut(i).assign(&hi_new);

                // Output: y_i = C_i · h_i + D · x_i
                let ci = self.c_proj.row(i);
                let yi: f64 = ci.dot(&hi_new) + self.d * xt[i];
                y[[t, i]] = yi;
            }
        }

        y
    }

    /// Parallel selective scan for multi-dimensional SSM.
    ///
    /// Pre-computes per-channel parameters, runs [`MultiDimScan::parallel_scan`]
    /// independently for each channel, then computes outputs.
    pub fn selective_scan_parallel(&self, x: &Array2<f64>) -> Array2<f64> {
        let length = x.shape()[0];
        let d_inner = self.input_dim;

        let mut y = Array2::zeros((length, d_inner));

        for i in 0..d_inner {
            // Pre-compute per-channel (A_t, B_t, C_i) over all timesteps
            let mut a_matrices = Vec::with_capacity(length);
            let mut b_vectors = Vec::with_capacity(length);

            for t in 0..length {
                let xt = x.row(t);
                let b_shared: Array1<f64> = self.b_proj.dot(&xt);
                let raw_delta: f64 = self.delta_weight.row(i).dot(&xt) + self.delta_bias[i];
                let delta = Self::softplus(raw_delta)
                    .max(self.delta_min)
                    .min(self.delta_max);

                let (a_bar, b_bar) = self.discretize(&b_shared, delta);
                a_matrices.push(a_bar);
                b_vectors.push(b_bar);
            }

            // Parallel scan → inclusive prefixes
            let prefixes = MultiDimScan::parallel_scan(&a_matrices, &b_vectors);

            // Compute outputs: h_t = B_combined[t] (since h₀ = 0)
            let ci = self.c_proj.row(i);
            for t in 0..length {
                let h_t = &prefixes[t].1;
                y[[t, i]] = ci.dot(h_t) + self.d * x[[t, i]];
            }
        }

        y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_selective_ssm_output_shape() {
        let n = 4;
        let d = 2;
        let a = Array2::eye(n) * -0.5;
        let ssm = SelectiveSSM::new(n, d, a);

        let x: Array2<f64> = Array2::zeros((8, d)).mapv(|_: f64| rand::random::<f64>() * 0.5);
        let y = ssm.selective_scan(&x);

        assert_eq!(y.shape(), &[8, d]);
    }

    #[test]
    fn test_softplus() {
        assert!((SelectiveSSM::softplus(0.0) - 0.6931).abs() < 1e-3);
        assert!((SelectiveSSM::softplus(10.0) - 10.0).abs() < 1e-3);
        assert!(SelectiveSSM::softplus(-10.0) > 0.0);
        assert!(SelectiveSSM::softplus(-10.0) < 1e-3);
    }

    #[test]
    fn test_multidim_selective_ssm_output_shape() {
        let n = 4;
        let d_inner = 3;
        let a = Array2::eye(n) * -0.5;
        let ssm = MultiDimSelectiveSSM::new(n, d_inner, a);

        let x: Array2<f64> = Array2::zeros((8, d_inner)).mapv(|_: f64| rand::random::<f64>() * 0.5);
        let y = ssm.selective_scan(&x);

        assert_eq!(y.shape(), &[8, d_inner]);
        // Each channel should produce different output (not all identical)
        let row0 = y.row(0);
        let mut all_same = true;
        for j in 1..d_inner {
            if (row0[j] - row0[0]).abs() > 1e-10 {
                all_same = false;
                break;
            }
        }
        // With per-channel Δ and C, outputs should differ (unlikely to be identical)
        // Only check if d_inner > 1 and inputs have variation
        if d_inner > 1 {
            // With random weights it's virtually impossible for all channels to match
            assert!(
                !all_same,
                "Multi-dim SSM channels should produce different outputs"
            );
        }
    }

    #[test]
    fn test_multidim_ssm_different_deltas() {
        let n = 2;
        let d_inner = 2;
        let a = Array2::eye(n) * -0.5;
        let mut ssm = MultiDimSelectiveSSM::new(n, d_inner, a);

        // Force different delta biases so channels behave differently
        ssm.delta_bias[0] = 0.5;
        ssm.delta_bias[1] = 0.01;

        let x: Array2<f64> = Array2::zeros((4, d_inner)).mapv(|_: f64| rand::random::<f64>() * 0.5);
        let y = ssm.selective_scan(&x);

        assert_eq!(y.shape(), &[4, d_inner]);
        // Output channels should differ
        let diff = (y[[3, 0]] - y[[3, 1]]).abs();
        assert!(
            diff > 1e-6,
            "Different Δ biases should give different outputs, got diff={}",
            diff
        );
    }

    #[test]
    fn test_selective_ssm_parallel_equivalence() {
        // Verify parallel scan matches sequential scan
        let n = 4;
        let d = 2;
        let a = Array2::eye(n) * -0.5;
        let ssm = SelectiveSSM::new(n, d, a);

        let x: Array2<f64> = Array2::zeros((8, d)).mapv(|_: f64| rand::random::<f64>() * 0.5);
        let y_seq = ssm.selective_scan(&x);
        let y_par = ssm.selective_scan_parallel(&x);

        assert_eq!(y_seq.shape(), y_par.shape());
        for i in 0..y_seq.shape()[0] {
            for j in 0..y_seq.shape()[1] {
                assert!(
                    (y_seq[[i, j]] - y_par[[i, j]]).abs() < 1e-10,
                    "Mismatch at [{},{}]: seq={}, par={}",
                    i,
                    j,
                    y_seq[[i, j]],
                    y_par[[i, j]]
                );
            }
        }
    }

    #[test]
    fn test_multidim_selective_ssm_parallel_equivalence() {
        let n = 3;
        let d_inner = 4;
        let a = Array2::eye(n) * -0.5;
        let ssm = MultiDimSelectiveSSM::new(n, d_inner, a);

        let x: Array2<f64> = Array2::zeros((6, d_inner)).mapv(|_: f64| rand::random::<f64>() * 0.5);
        let y_seq = ssm.selective_scan(&x);
        let y_par = ssm.selective_scan_parallel(&x);

        assert_eq!(y_seq.shape(), y_par.shape());
        for i in 0..y_seq.shape()[0] {
            for j in 0..y_seq.shape()[1] {
                assert!(
                    (y_seq[[i, j]] - y_par[[i, j]]).abs() < 1e-10,
                    "Mismatch at [{},{}]: seq={}, par={}",
                    i,
                    j,
                    y_seq[[i, j]],
                    y_par[[i, j]]
                );
            }
        }
    }
}
