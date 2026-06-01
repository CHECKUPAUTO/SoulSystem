//! MambaModel: stacked Mamba blocks with embedding / output heads.
//!
//! Full model assembly:
//!
//!   Tokens → Embedding → [MambaBlock × N] → RMSNorm → Output Projection
//!
//! Supports language modeling (token prediction) as the primary use case,
//! with configurable vocabulary size, number of layers, and hidden dimensions.

use ndarray::{Array1, Array2};

use crate::layer::{GateType, MambaBlock, MambaConfig};

/// Configuration for the full Mamba model.
#[derive(Debug, Clone)]
pub struct MambaModelConfig {
    /// Vocabulary size
    pub vocab_size: usize,
    /// Number of Mamba blocks (layers)
    pub n_layers: usize,
    /// Model dimension (D)
    pub d_model: usize,
    /// SSM state dimension (N)
    pub state_dim: usize,
    /// Expansion factor
    pub expand_factor: usize,
    /// Convolution kernel size
    pub conv_kernel: usize,
    /// Layer norm epsilon
    pub eps: f64,
    /// Tie embedding weights with output projection
    pub weight_tied: bool,
    /// Gate type (SiLU or Grouped GQA-style)
    pub gate_type: GateType,
}

impl Default for MambaModelConfig {
    fn default() -> Self {
        Self {
            vocab_size: 32000,
            n_layers: 6,
            d_model: 64,
            state_dim: 16,
            expand_factor: 2,
            conv_kernel: 4,
            eps: 1e-5,
            weight_tied: false,
            gate_type: GateType::SiLU,
        }
    }
}

/// Mamba language model.
///
/// Stacked Mamba blocks with token embedding and output projection.
pub struct MambaModel {
    /// Model configuration
    pub config: MambaModelConfig,
    /// Token embedding matrix (vocab_size, d_model)
    pub embedding: Array2<f64>,
    /// Stacked Mamba blocks
    pub layers: Vec<MambaBlock>,
    /// Final RMSNorm scale
    pub norm_scale: Array1<f64>,
    /// Output projection (vocab_size, d_model) — typically tied with embedding
    pub output_proj: Array2<f64>,
}

impl MambaModel {
    /// Create a new Mamba model with random initialization.
    ///
    /// # Arguments
    ///
    /// * `config` - Model configuration
    pub fn new(config: MambaModelConfig) -> Self {
        let d = config.d_model;
        let vocab = config.vocab_size;

        // Initialize embedding matrix
        let mut rng = rand::thread_rng();
        let scale = (1.0 / d as f64).sqrt();
        let embedding: Array2<f64> =
            Array2::zeros((vocab, d)).mapv(|_: f64| rand::Rng::gen_range(&mut rng, -scale..scale));

        // Create stacked Mamba blocks
        let layers: Vec<MambaBlock> = (0..config.n_layers)
            .map(|_| {
                MambaBlock::new(MambaConfig {
                    d_model: config.d_model,
                    state_dim: config.state_dim,
                    expand_factor: config.expand_factor,
                    conv_kernel: config.conv_kernel,
                    dropout: 0.0,
                    eps: config.eps,
                    gate_type: config.gate_type,
                })
            })
            .collect();

        // Output projection (tied weights option uses embedding^T)
        let output_proj: Array2<f64> = if config.weight_tied {
            embedding.clone()
        } else {
            Array2::zeros((vocab, d)).mapv(|_: f64| rand::Rng::gen_range(&mut rng, -scale..scale))
        };
        let norm_scale = Array1::from_elem(d, 1.0);

        Self {
            config,
            embedding,
            layers,
            norm_scale,
            output_proj,
        }
    }

    /// RMS normalization.
    fn rms_norm(&self, x: &Array2<f64>) -> Array2<f64> {
        let (rows, cols) = (x.shape()[0], x.shape()[1]);
        let mut out = Array2::zeros((rows, cols));

        for i in 0..rows {
            let row = x.row(i);
            let mean_sq: f64 = row.iter().map(|v| v * v).sum::<f64>() / cols as f64;
            let rms = (mean_sq + self.config.eps).sqrt();
            for j in 0..cols {
                out[[i, j]] = (row[j] / rms) * self.norm_scale[j];
            }
        }

        out
    }

    /// Embed token IDs into dense vectors.
    ///
    /// # Arguments
    ///
    /// * `input_ids` - Token IDs (L,)
    ///
    /// # Returns
    ///
    /// Embedding vectors (L, D)
    fn embed(&self, input_ids: &[usize]) -> Array2<f64> {
        let l = input_ids.len();
        let d = self.config.d_model;
        let mut out = Array2::zeros((l, d));

        for (i, &token_id) in input_ids.iter().enumerate() {
            let token_id = token_id.min(self.config.vocab_size - 1);
            let embed_row = self.embedding.row(token_id);
            out.row_mut(i).assign(&embed_row);
        }

        out
    }

    /// Forward pass through the entire model.
    ///
    /// # Arguments
    ///
    /// * `input_ids` - Token IDs (L,)
    ///
    /// # Returns
    ///
    /// Logits (L, vocab_size) — one distribution per position
    pub fn forward(&self, input_ids: &[usize]) -> Array2<f64> {
        // Embed tokens
        let mut h = self.embed(input_ids);

        // Pass through each Mamba block
        for layer in &self.layers {
            h = layer.forward(&h);
        }

        // Final normalization
        h = self.rms_norm(&h);

        // Project to vocabulary
        let logits = self.output_proj.dot(&h.t()).t().to_owned();

        logits
    }

    /// Generate the next token given a sequence.
    ///
    /// Simple greedy generation — picks the token with the highest logit.
    ///
    /// # Arguments
    ///
    /// * `input_ids` - Token IDs (L,)
    ///
    /// # Returns
    ///
    /// Next token ID
    pub fn generate_next(&self, input_ids: &[usize]) -> usize {
        let logits = self.forward(input_ids);
        let last_row = logits.row(logits.shape()[0] - 1);

        // Greedy: argmax
        last_row
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            .map(|(idx, _)| idx)
            .unwrap_or(0)
    }

    /// Get the number of parameters (estimated).
    pub fn param_count(&self) -> usize {
        let embed_params = self.embedding.len();
        // With weight tying, output projection shares embedding weights
        let out_params = if self.config.weight_tied {
            0
        } else {
            self.output_proj.len()
        };

        let mut layer_params = 0usize;
        for layer in &self.layers {
            let d = layer.config.d_model;
            let d_inner = layer.d_inner;
            let s = layer.config.state_dim;
            let k = layer.config.conv_kernel;

            // in_proj: (2*d_inner, d)
            layer_params += 2 * d_inner * d;
            // conv_weight: (d_inner, k)
            layer_params += d_inner * k;
            // conv_bias: d_inner
            layer_params += d_inner;
            // SSM: A (s, s) + B_proj (s, d_inner) + C_proj (s, d_inner) + delta_proj (d_inner)
            layer_params += s * s + 2 * s * d_inner + d_inner;
            // out_proj: (d, d_inner)
            layer_params += d * d_inner;
            // norm: 2 * d (scale + bias)
            layer_params += 2 * d;
        }

        embed_params + out_params + self.config.n_layers + layer_params
    }

    // ─── Inference Cache ───────────────────────────────────────────────────

    /// Create a fresh inference cache for autoregressive generation.
    ///
    /// The cache holds per-layer SSM states and convolution histories.
    pub fn new_cache(&self) -> InferenceCache {
        let n_layers = self.layers.len();
        let d_inner = self.config.d_model * self.config.expand_factor;
        let state_dim = self.config.state_dim;
        let kernel = self.config.conv_kernel;

        let ssm_states = (0..n_layers).map(|_| Array1::zeros(state_dim)).collect();
        let conv_caches = (0..n_layers)
            .map(|_| Array2::zeros((d_inner, kernel.saturating_sub(1))))
            .collect();

        InferenceCache {
            ssm_states,
            conv_caches,
            last_token: 0,
            last_embedding: None,
        }
    }

    /// Generate the next token using cached autoregressive state.
    ///
    /// # Warmup (prefix fill)
    ///
    /// Pass the full `input_ids` (>1 token) to warm up the cache.
    /// Runs the full forward pass and stores per-layer SSM states and
    /// convolution histories.
    ///
    /// # Single-token generation
    ///
    /// Pass a single `input_ids` token (the previously generated one).
    /// Uses cached per-layer states via `forward_step` to produce the
    /// next token — O(1) per step instead of O(L).
    ///
    /// # Auto-continuation
    ///
    /// Pass `&[]` to continue from the last generated token automatically.
    ///
    /// Returns (next_token_id, logits_for_last_position).
    pub fn generate_cached(
        &self,
        input_ids: &[usize],
        cache: &mut InferenceCache,
    ) -> (usize, Array1<f64>) {
        let token_ids = if !input_ids.is_empty() {
            input_ids.to_vec()
        } else {
            vec![cache.last_token]
        };

        if token_ids.len() > 1 {
            // ── Warmup / prefix fill ──────────────────────────────────
            let (logits, final_states, final_convs) = self.forward_with_states(&token_ids);

            // Store per-layer states
            for (i, state) in final_states.iter().enumerate() {
                if i < cache.ssm_states.len() {
                    cache.ssm_states[i].assign(state);
                }
            }
            for (i, conv) in final_convs.iter().enumerate() {
                if i < cache.conv_caches.len() {
                    cache.conv_caches[i].assign(conv);
                }
            }

            let last_row = logits.row(logits.shape()[0] - 1);
            let next = last_row
                .iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
                .map(|(idx, _)| idx)
                .unwrap_or(0);

            cache.last_token = next;
            cache.last_embedding = Some(self.embed(&[next]).row(0).to_owned());

            (next, last_row.to_owned())
        } else {
            // ── Single-token generation with cache ────────────────────
            let token = token_ids[0];

            // Embed single token
            let mut h = self.embed(&[token]);

            // Run through each layer using cached forward_step
            for (i, layer) in self.layers.iter().enumerate() {
                let x_t = h.row(0).to_owned();
                let d = layer.config.d_model;
                let d_inner = layer.d_inner;
                let k = layer.config.conv_kernel;

                // RMSNorm on single vector
                let mean_sq: f64 = x_t.iter().map(|v| v * v).sum::<f64>() / d as f64;
                let rms = (mean_sq + layer.config.eps).sqrt();
                let mut normed = Array1::zeros(d);
                for j in 0..d {
                    normed[j] = (x_t[j] / rms) * layer.norm_scale[j] + layer.norm_bias[j];
                }

                // Input projection
                let proj = layer.in_proj.dot(&normed);
                let x_ssm_t = proj.slice(ndarray::s![..d_inner]).to_owned();
                let x_gate_raw_t = proj.slice(ndarray::s![d_inner..]).to_owned();

                // Conv1D step using cache
                let mut conv_out = Array1::zeros(d_inner);
                for c in 0..d_inner {
                    let mut sum = layer.conv_bias[c];
                    sum += layer.conv_weight[[c, k - 1]] * x_ssm_t[c];
                    for ki in 0..(k - 1) {
                        sum += layer.conv_weight[[c, ki]] * cache.conv_caches[i][[c, ki]];
                    }
                    conv_out[c] = sum;
                }

                // Update conv cache: shift left, insert new input at end
                for c in 0..d_inner {
                    for j in 0..(k.saturating_sub(2)) {
                        cache.conv_caches[i][[c, j]] = cache.conv_caches[i][[c, j + 1]];
                    }
                    if k > 1 {
                        cache.conv_caches[i][[c, k - 2]] = x_ssm_t[c];
                    }
                }

                // SiLU activation
                let act_t = conv_out.mapv(|x| {
                    let sig = 1.0 / (1.0 + (-x).exp());
                    x * sig
                });

                // Selective SSM step
                let _n = layer.selective_ssm.state_dim;
                let b_vec: Array1<f64> = layer.selective_ssm.b_proj.dot(&act_t);
                let c_vec: Array1<f64> = layer.selective_ssm.c_proj.dot(&act_t);

                let raw_delta: f64 = layer
                    .selective_ssm
                    .delta_proj
                    .iter()
                    .zip(act_t.iter())
                    .map(|(w, a)| w * a)
                    .sum();
                let delta = if raw_delta > 20.0 {
                    raw_delta
                } else if raw_delta < -20.0 {
                    raw_delta.exp()
                } else {
                    (1.0 + raw_delta.exp()).ln()
                }
                .max(layer.selective_ssm.delta_min)
                .min(layer.selective_ssm.delta_max);

                // Discretize
                let (a_bar, b_bar_1d) =
                    crate::ssm::discretize_zoh(&layer.selective_ssm.a, &b_vec, delta);

                // State update using cache
                let h_new = a_bar.dot(&cache.ssm_states[i]) + &b_bar_1d;
                cache.ssm_states[i].assign(&h_new);

                // Output: C · h + D
                let ssm_val: f64 = c_vec.dot(&h_new) + layer.selective_ssm.d;
                let ssm_out = Array1::from_elem(d_inner, ssm_val);

                // Gate: SiLU then expand if grouped
                let gate_act_t = x_gate_raw_t.mapv(|x| {
                    let sig = 1.0 / (1.0 + (-x).exp());
                    x * sig
                });
                let gate_t = match &layer.gate_expand {
                    Some(expand) => expand.dot(&gate_act_t),
                    None => gate_act_t,
                };
                let gated = ssm_out * gate_t;

                // Output projection + residual
                let out_1d = layer.out_proj.dot(&gated) + &x_t;
                h = out_1d.into_shape_with_order((1, d)).unwrap();
            }

            // Final RMSNorm + output projection
            h = self.rms_norm(&h);
            let logits = self.output_proj.dot(&h.t()).t().to_owned();

            let next = logits
                .row(0)
                .iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
                .map(|(idx, _)| idx)
                .unwrap_or(0);

            cache.last_token = next;
            cache.last_embedding = Some(self.embed(&[next]).row(0).to_owned());

            (next, logits.row(0).to_owned())
        }
    }

    /// Forward pass that also returns per-layer final states.
    fn forward_with_states(
        &self,
        input_ids: &[usize],
    ) -> (Array2<f64>, Vec<Array1<f64>>, Vec<Array2<f64>>) {
        let mut h = self.embed(input_ids);

        let mut final_states = Vec::with_capacity(self.layers.len());
        let mut final_convs = Vec::with_capacity(self.layers.len());

        for layer in &self.layers {
            let d_inner_l = layer.d_inner;
            let k = layer.config.conv_kernel;

            // Extract conv cache: last (k-1) SSM-path inputs
            let normed = layer.rms_norm(&h);
            let proj = layer.in_proj.dot(&normed.t());
            let proj_t = proj.t().to_owned();
            let x_ssm_input = proj_t.slice(ndarray::s![.., ..d_inner_l]).to_owned();

            // Fill conv cache from the last (k-1) positions
            let mut conv_cache = Array2::zeros((d_inner_l, k.saturating_sub(1)));
            let len = x_ssm_input.shape()[0];
            if k > 1 {
                for c in 0..d_inner_l {
                    for j in 0..(k - 1) {
                        let src_idx = len as isize - (k as isize - 1) + j as isize;
                        if src_idx >= 0 && src_idx < len as isize {
                            conv_cache[[c, j]] = x_ssm_input[[src_idx as usize, c]];
                        }
                    }
                }
            }
            final_convs.push(conv_cache);

            // Apply conv1d + SiLU to get actual SSM input, then extract final state
            let x_conv = layer.conv1d(&x_ssm_input);
            let x_act = crate::layer::MambaBlock::silu_array(&x_conv);
            let final_state = layer.selective_ssm.selective_scan_final_state(&x_act);
            final_states.push(final_state);

            h = layer.forward(&h);
        }

        h = self.rms_norm(&h);
        let logits = self.output_proj.dot(&h.t()).t().to_owned();

        (logits, final_states, final_convs)
    }
}

/// Cache holding per-layer state for autoregressive generation.
pub struct InferenceCache {
    /// SSM hidden state per layer (N,)
    pub ssm_states: Vec<Array1<f64>>,
    /// Convolution history per layer (d_inner, kernel-1)
    pub conv_caches: Vec<Array2<f64>>,
    /// Last generated token (for empty-slice continuation)
    pub last_token: usize,
    /// Cached embedding of the last token (D,)
    pub last_embedding: Option<Array1<f64>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_forward_shape() {
        let config = MambaModelConfig {
            vocab_size: 100,
            n_layers: 2,
            d_model: 16,
            state_dim: 4,
            expand_factor: 2,
            conv_kernel: 3,
            eps: 1e-5,
            weight_tied: false,
            gate_type: GateType::SiLU,
        };

        let model = MambaModel::new(config);
        let input_ids: Vec<usize> = (0..8).map(|i| i % 100).collect();

        let logits = model.forward(&input_ids);
        assert_eq!(logits.shape(), &[8, 100]);
    }

    #[test]
    fn test_generate_next() {
        let config = MambaModelConfig {
            vocab_size: 100,
            n_layers: 1,
            d_model: 8,
            state_dim: 2,
            expand_factor: 2,
            conv_kernel: 3,
            eps: 1e-5,
            weight_tied: false,
            gate_type: GateType::SiLU,
        };

        let model = MambaModel::new(config);
        let input_ids = vec![5, 10, 15];
        let next_token = model.generate_next(&input_ids);

        assert!(next_token < 100);
    }

    #[test]
    fn test_weight_tied() {
        let config = MambaModelConfig {
            vocab_size: 100,
            n_layers: 1,
            d_model: 8,
            state_dim: 2,
            expand_factor: 2,
            conv_kernel: 3,
            eps: 1e-5,
            weight_tied: true,
            gate_type: GateType::SiLU,
        };

        let model = MambaModel::new(config);
        let input_ids = vec![5, 10, 15];
        let logits = model.forward(&input_ids);

        assert_eq!(logits.shape(), &[3, 100]);

        // With weight tying, output_proj should match embedding
        let eps = 1e-10;
        for i in 0..model.embedding.shape()[0] {
            for j in 0..model.embedding.shape()[1] {
                assert!(
                    (model.output_proj[[i, j]] - model.embedding[[i, j]]).abs() < eps,
                    "weight_tied: output_proj must equal embedding at [{},{}]",
                    i,
                    j
                );
            }
        }
    }

    #[test]
    fn test_cached_generation_equivalence() {
        // Verify generate_cached produces the same tokens as generate_next
        let config = MambaModelConfig {
            vocab_size: 100,
            n_layers: 2,
            d_model: 16,
            state_dim: 4,
            expand_factor: 2,
            conv_kernel: 3,
            eps: 1e-5,
            weight_tied: false,
            gate_type: GateType::SiLU,
        };

        let model = MambaModel::new(config);
        let prefix: Vec<usize> = (0..6).map(|i| i % 100).collect();

        // Reference: generate_next called repeatedly on growing context
        let mut ref_tokens = Vec::new();
        let mut ctx = prefix.clone();
        for _ in 0..4 {
            let next = model.generate_next(&ctx);
            ref_tokens.push(next);
            ctx.push(next);
        }

        // Verify forward_with_states produces same logits as forward
        let ref_logits = model.forward(&prefix);
        let (ws_logits, _, _) = model.forward_with_states(&prefix);
        assert_eq!(ref_logits.shape(), ws_logits.shape());
        for r in 0..ref_logits.shape()[0] {
            for c in 0..ref_logits.shape()[1] {
                assert!(
                    (ref_logits[[r, c]] - ws_logits[[r, c]]).abs() < 1e-10,
                    "Logit mismatch at [{},{}]: forward={}, ws={}",
                    r,
                    c,
                    ref_logits[[r, c]],
                    ws_logits[[r, c]]
                );
            }
        }

        // Cached: warmup + single-token steps
        let mut cache = model.new_cache();
        let mut cached_tokens = Vec::new();
        let (next, _) = model.generate_cached(&prefix, &mut cache);
        cached_tokens.push(next);
        // First token must match
        assert_eq!(
            cached_tokens[0], ref_tokens[0],
            "First token mismatch: cached={}, ref={}",
            cached_tokens[0], ref_tokens[0]
        );
        let mut current = next;
        for _ in 0..3 {
            let (token, _) = model.generate_cached(&[current], &mut cache);
            cached_tokens.push(token);
            current = token;
        }

        assert_eq!(ref_tokens.len(), cached_tokens.len());
        for i in 0..ref_tokens.len() {
            assert_eq!(
                cached_tokens[i], ref_tokens[i],
                "Token mismatch at position {}: cached={}, ref={}",
                i, cached_tokens[i], ref_tokens[i]
            );
        }
    }

    #[test]
    fn test_cached_generation_auto_continue() {
        // Verify &[] auto-continuation works
        let config = MambaModelConfig {
            vocab_size: 100,
            n_layers: 1,
            d_model: 8,
            state_dim: 2,
            expand_factor: 2,
            conv_kernel: 3,
            eps: 1e-5,
            weight_tied: false,
            gate_type: GateType::SiLU,
        };

        let model = MambaModel::new(config);
        let prefix = vec![5, 10, 15];

        let mut cache = model.new_cache();
        let (token1, _) = model.generate_cached(&prefix, &mut cache);

        // Auto-continue with empty slice
        let (token2, _) = model.generate_cached(&[], &mut cache);
        let (token3, _) = model.generate_cached(&[], &mut cache);

        // These should produce valid tokens
        assert!(token1 < 100);
        assert!(token2 < 100);
        assert!(token3 < 100);
    }
}
