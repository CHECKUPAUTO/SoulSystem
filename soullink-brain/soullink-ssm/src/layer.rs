//! Mamba block: gated SSM layer with convolution + activation.
//!
//! Structure of a single Mamba block:
//!
//!   x → LayerNorm → Conv1D → SiLU → SelectiveSSM → SiLU → Gate → Output
//!       ↑                                               |
//!       └─────────────── Residual ──────────────────────┘
//!
//! The block combines:
//! 1. A depthwise 1D convolution for local context
//! 2. A selective SSM for long-range dependencies
//! 3. Gated activation with SiLU (Swish)
//! 4. Residual connection with layer normalization

use ndarray::{Array1, Array2, Array3};

use crate::hippo::HiPPOInitializer;
use crate::selective::SelectiveSSM;
use crate::ssm;

/// Re-export SSMKernel for non-selective SSM use.
pub use crate::ssm::SSMKernel;

/// Gate type for the Mamba block's gating mechanism.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GateType {
    /// Standard element-wise SiLU gate.
    SiLU,
    /// Grouped gating: compress gate to d_inner/num_groups, apply SiLU, expand back.
    /// Saves parameters while maintaining expressiveness.
    Grouped { num_groups: usize },
}

/// Configuration for a Mamba block.
#[derive(Debug, Clone)]
pub struct MambaConfig {
    /// Model dimension (D)
    pub d_model: usize,
    /// State dimension (N)
    pub state_dim: usize,
    /// Expansion factor (inner dimension = d_model * expand)
    pub expand_factor: usize,
    /// Kernel size for depthwise 1D convolution
    pub conv_kernel: usize,
    /// Dropout probability
    pub dropout: f64,
    /// Layer norm epsilon
    pub eps: f64,
    /// Gate type (SiLU or Grouped GQA-style)
    pub gate_type: GateType,
}

impl Default for MambaConfig {
    fn default() -> Self {
        Self {
            d_model: 64,
            state_dim: 16,
            expand_factor: 2,
            conv_kernel: 4,
            dropout: 0.0,
            eps: 1e-5,
            gate_type: GateType::SiLU,
        }
    }
}

/// A single Mamba block combining conv + selective SSM + gate.
pub struct MambaBlock {
    /// Configuration
    pub config: MambaConfig,
    /// Inner dimension after expansion
    pub d_inner: usize,
    /// Gate dimension (d_inner for SiLU, d_inner/num_groups for Grouped/GQA)
    pub gate_dim: usize,
    /// Layer normalization weights (scale, bias)
    pub norm_scale: Array1<f64>,
    pub norm_bias: Array1<f64>,
    /// Input projection weights (d_model → d_inner + gate_dim)
    pub in_proj: Array2<f64>,
    /// Gate expansion weight — only used with Grouped gating (d_inner, gate_dim)
    pub gate_expand: Option<Array2<f64>>,
    /// Convolution weights (depthwise, d_inner, conv_kernel)
    pub conv_weight: Array2<f64>,
    /// Convolution bias
    pub conv_bias: Array1<f64>,
    /// Selective SSM (inner → state → inner)
    pub selective_ssm: SelectiveSSM,
    /// Output projection (d_model, d_inner)
    pub out_proj: Array2<f64>,
}

impl MambaBlock {
    /// Create a new Mamba block with HiPPO-initialized SSM.
    ///
    /// # Arguments
    ///
    /// * `config` - Block configuration
    pub fn new(config: MambaConfig) -> Self {
        let d_inner = config.d_model * config.expand_factor;
        let d = config.d_model;

        // Gate dimension depends on gate type
        let gate_dim = match config.gate_type {
            GateType::SiLU => d_inner,
            GateType::Grouped { num_groups } => {
                assert!(
                    d_inner % num_groups == 0,
                    "d_inner ({}) must be divisible by num_groups ({})",
                    d_inner,
                    num_groups
                );
                d_inner / num_groups
            }
        };

        // HiPPO-initialize the SSM state matrix A
        let a = HiPPOInitializer::legs(config.state_dim);

        // Initialize projections with small random values
        let mut rng = rand::thread_rng();
        let scale = (1.0 / d as f64).sqrt();

        let in_proj: Array2<f64> = Array2::zeros((d_inner + gate_dim, d))
            .mapv(|_: f64| rand::Rng::gen_range(&mut rng, -scale..scale));

        let gate_expand = match config.gate_type {
            GateType::SiLU => None,
            GateType::Grouped { .. } => {
                let expand: Array2<f64> = Array2::zeros((d_inner, gate_dim))
                    .mapv(|_: f64| rand::Rng::gen_range(&mut rng, -scale..scale));
                Some(expand)
            }
        };

        let conv_weight: Array2<f64> = Array2::zeros((d_inner, config.conv_kernel))
            .mapv(|_: f64| rand::Rng::gen_range(&mut rng, -0.1..0.1));
        let conv_bias = Array1::zeros(d_inner);

        let out_proj: Array2<f64> = Array2::zeros((d, d_inner))
            .mapv(|_: f64| rand::Rng::gen_range(&mut rng, -scale..scale));

        let selective_ssm = SelectiveSSM::new(config.state_dim, d_inner, a);

        Self {
            norm_scale: Array1::from_elem(d, 1.0),
            norm_bias: Array1::from_elem(d, 0.0),
            in_proj,
            gate_expand,
            conv_weight,
            conv_bias,
            selective_ssm,
            out_proj,
            d_inner,
            gate_dim,
            config,
        }
    }

    /// Layer normalization (RMSNorm variant).
    pub(crate) fn rms_norm(&self, x: &Array2<f64>) -> Array2<f64> {
        let (rows, cols) = (x.shape()[0], x.shape()[1]);
        let mut out = Array2::zeros((rows, cols));

        for i in 0..rows {
            let row = x.row(i);
            let mean_sq: f64 = row.iter().map(|v| v * v).sum::<f64>() / cols as f64;
            let rms = (mean_sq + self.config.eps).sqrt();
            for j in 0..cols {
                out[[i, j]] = (row[j] / rms) * self.norm_scale[j] + self.norm_bias[j];
            }
        }

        out
    }

    /// SiLU (Swish) activation: x * sigmoid(x)
    pub(crate) fn silu(x: f64) -> f64 {
        let sig = 1.0 / (1.0 + (-x).exp());
        x * sig
    }

    /// Apply SiLU element-wise to an array.
    pub(crate) fn silu_array(x: &Array2<f64>) -> Array2<f64> {
        x.mapv(Self::silu)
    }

    /// 1D causal convolution along the sequence dimension.
    ///
    /// Applies padding on the left to maintain sequence length:
    ///   y[t] = Σ_k w[k] · x[t - k + pad]
    /// for k where (t - k + pad) is in bounds.
    pub(crate) fn conv1d(&self, x: &Array2<f64>) -> Array2<f64> {
        let length = x.shape()[0];
        let channels = self.d_inner;
        let k = self.config.conv_kernel;
        let _pad = k - 1;

        let mut out = Array2::zeros((length, channels));

        for c in 0..channels {
            for t in 0..length {
                let mut sum = self.conv_bias[c];
                for ki in 0..k {
                    let src = (t as isize) - (ki as isize);
                    if src >= 0 {
                        sum += self.conv_weight[[c, ki]] * x[[src as usize, c]];
                    }
                }
                out[[t, c]] = sum;
            }
        }

        out
    }

    /// Forward pass through the Mamba block.
    ///
    /// # Arguments
    ///
    /// * `x` - Input sequence (L, D)
    ///
    /// # Returns
    ///
    /// Output sequence (L, D)
    pub fn forward(&self, x: &Array2<f64>) -> Array2<f64> {
        let _d = self.config.d_model;
        let d_inner = self.d_inner;

        // Residual connection
        let residual = x.clone();

        // Layer normalization
        let normed = self.rms_norm(x);

        // Input projection: x → [x_ssm; x_gate]
        let proj = self.in_proj.dot(&normed.t());
        // proj is (d_inner+gate_dim, L), transpose to (L, d_inner+gate_dim)
        let proj_t = proj.t().to_owned();

        // Split into SSM and gate paths
        let x_ssm_input = proj_t.slice(ndarray::s![.., ..d_inner]).to_owned();
        let x_gate_raw = proj_t.slice(ndarray::s![.., d_inner..]).to_owned();

        // 1D convolution on SSM path
        let x_conv = self.conv1d(&x_ssm_input);

        // SiLU activation
        let x_act = Self::silu_array(&x_conv);

        // Selective SSM
        let x_ssm = self.selective_ssm.selective_scan(&x_act);

        // Gate: SiLU then expand if grouped
        let gate_act = Self::silu_array(&x_gate_raw);
        let gate = match &self.gate_expand {
            Some(expand) => {
                // Grouped/GQA: expand compressed gate back to d_inner
                expand.dot(&gate_act.t()).t().to_owned()
            }
            None => gate_act,
        };
        let gated = &x_ssm * &gate;

        // Output projection
        let out = self.out_proj.dot(&gated.t()).t().to_owned();

        // Residual connection
        out + residual
    }

    /// Process a batch of sequences.
    ///
    /// # Arguments
    ///
    /// * `x` - Input batch (B, L, D) where B = batch size
    ///
    /// # Returns
    ///
    /// Output batch (B, L, D)
    pub fn forward_batch(&self, x: &Array3<f64>) -> Array3<f64> {
        let shape = x.shape();
        let b = shape[0];
        let l = shape[1];
        let d = shape[2];

        let mut out = Array3::zeros((b, l, d));

        for i in 0..b {
            let slice = x.slice(ndarray::s![i, .., ..]).to_owned();
            let result = self.forward(&slice);
            out.slice_mut(ndarray::s![i, .., ..]).assign(&result);
        }

        out
    }

    /// Single-token forward step with cached state for autoregressive generation.
    ///
    /// Processes one timestep using previously cached SSM hidden state and
    /// convolution history, updating both caches in place.
    ///
    /// The SSM produces a single scalar output which is broadcast across
    /// all d_inner channels; per-channel variation comes from the gate.
    ///
    /// # Arguments
    ///
    /// * `x_t` - Single input vector (D,)
    /// * `ssm_state` - SSM hidden state (N,) — updated in place
    /// * `conv_cache` - Conv history (d_inner, kernel-1) — updated in place
    ///
    /// # Returns
    ///
    /// Output vector (D,)
    pub fn forward_step(
        &self,
        x_t: &Array1<f64>,
        ssm_state: &mut Array1<f64>,
        conv_cache: &mut Array2<f64>,
    ) -> Array1<f64> {
        let d = self.config.d_model;
        let d_inner = self.d_inner;
        let k = self.config.conv_kernel;

        // RMSNorm on single vector
        let mean_sq: f64 = x_t.iter().map(|v| v * v).sum::<f64>() / d as f64;
        let rms = (mean_sq + self.config.eps).sqrt();
        let mut normed = Array1::zeros(d);
        for j in 0..d {
            normed[j] = (x_t[j] / rms) * self.norm_scale[j] + self.norm_bias[j];
        }

        // Input projection: x → [x_ssm, x_gate]
        let proj = self.in_proj.dot(&normed); // (d_inner+gate_dim,)

        let x_ssm_t = proj.slice(ndarray::s![..d_inner]).to_owned();
        let x_gate_raw_t = proj.slice(ndarray::s![d_inner..]).to_owned();

        // Conv1D step with cache:
        // y[c] = bias[c] + w[c, k-1]·x_t + Σ_{i=0}^{k-2} w[c, i]·cache[c, i]
        // where cache[0] = x[t-(k-1)], cache[1] = x[t-(k-2)], ..., cache[k-2] = x[t-1]
        let mut conv_out = Array1::zeros(d_inner);
        for c in 0..d_inner {
            let mut sum = self.conv_bias[c];
            // Current input weighted by rightmost kernel element (most recent)
            sum += self.conv_weight[[c, k - 1]] * x_ssm_t[c];
            // Past inputs from cache (w[0] ← oldest, w[k-2] ← most recent past)
            for ki in 0..(k - 1) {
                sum += self.conv_weight[[c, ki]] * conv_cache[[c, ki]];
            }
            conv_out[c] = sum;
        }

        // Update conv cache: shift left, insert new input at end
        for c in 0..d_inner {
            for j in 0..(k - 2) {
                conv_cache[[c, j]] = conv_cache[[c, j + 1]];
            }
            if k > 1 {
                conv_cache[[c, k - 2]] = x_ssm_t[c];
            }
        }

        // SiLU activation on conv output
        let act_t = conv_out.mapv(Self::silu);

        // Selective SSM step (single N-dim state)
        // B(x) = B_proj @ x → (N,)
        let n = self.selective_ssm.state_dim;
        let b_vec: Array1<f64> = self.selective_ssm.b_proj.dot(&act_t);
        // C(x) = C_proj @ x → (N,)
        let c_vec: Array1<f64> = self.selective_ssm.c_proj.dot(&act_t);
        // Δ(x) = sum(delta_proj * x)
        let raw_delta: f64 = self
            .selective_ssm
            .delta_proj
            .iter()
            .zip(act_t.iter())
            .map(|(w, a)| w * a)
            .sum();
        let delta = SelectiveSSM::softplus(raw_delta)
            .max(self.selective_ssm.delta_min)
            .min(self.selective_ssm.delta_max);

        // Discretize
        let (a_bar, b_bar_mat) = self.discretize_step(&b_vec, delta, n);
        let b_bar = b_bar_mat.column(0).to_owned();

        // State update: h = Ā · h + B̄
        let h_new = a_bar.dot(ssm_state) + &b_bar;
        ssm_state.assign(&h_new);

        // Output: y = C · h + D
        let ssm_val: f64 = c_vec.dot(&h_new) + self.selective_ssm.d;
        let ssm_out = Array1::from_elem(d_inner, ssm_val);

        // Gate: SiLU then expand if grouped
        let gate_act_t = x_gate_raw_t.mapv(Self::silu);
        let gate_t = match &self.gate_expand {
            Some(expand) => expand.dot(&gate_act_t),
            None => gate_act_t,
        };
        let gated = ssm_out * gate_t;

        // Output projection
        let out = self.out_proj.dot(&gated);

        // Residual connection
        out + x_t
    }

    /// Lightweight discretization for single-step inference.
    fn discretize_step(
        &self,
        b_vec: &Array1<f64>,
        delta: f64,
        n: usize,
    ) -> (Array2<f64>, Array2<f64>) {
        let (a_bar, b_bar_1d) = ssm::discretize_zoh(&self.selective_ssm.a, b_vec, delta);
        let mut b_bar = Array2::zeros((n, 1));
        for i in 0..n {
            b_bar[[i, 0]] = b_bar_1d[i];
        }
        (a_bar, b_bar)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mamba_block_output_shape() {
        let config = MambaConfig {
            d_model: 16,
            state_dim: 4,
            expand_factor: 2,
            conv_kernel: 4,
            dropout: 0.0,
            eps: 1e-5,
            gate_type: GateType::SiLU,
        };

        let block = MambaBlock::new(config);
        let x: Array2<f64> = Array2::zeros((8, 16)).mapv(|_: f64| rand::random::<f64>() * 0.5);
        let y = block.forward(&x);

        assert_eq!(y.shape(), &[8, 16]);
    }

    #[test]
    fn test_mamba_block_batch() {
        let config = MambaConfig {
            d_model: 8,
            state_dim: 2,
            expand_factor: 2,
            conv_kernel: 3,
            dropout: 0.0,
            eps: 1e-5,
            gate_type: GateType::SiLU,
        };

        let block = MambaBlock::new(config);
        let x: Array3<f64> = Array3::zeros((2, 6, 8)).mapv(|_: f64| rand::random::<f64>() * 0.3);
        let y = block.forward_batch(&x);

        assert_eq!(y.shape(), &[2, 6, 8]);
    }
}
