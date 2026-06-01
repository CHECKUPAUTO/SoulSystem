//! SSM Cortex — Mamba-style cognitive cortex for SoulLink Brain.
//!
//! Bridges the biological neural simulation with a MambaModel (S6 / Mamba)
//! for structured sequence processing. The cortex:
//!
//! 1. Encodes brain module-level stats into feature vectors
//! 2. Projects features into model dimension via a learned input projection
//! 3. Maintains a ring buffer of recent brain state embeddings
//! 4. Runs the full MambaModel (stacked Mamba blocks + RMSNorm) over the sequence
//! 5. Decodes the SSM output into modulation signals for the brain
//! 6. Learns to predict the next brain state (autoregressive prediction target)

use std::collections::VecDeque;
use serde::{Deserialize, Serialize};
use ndarray::{Array1, Array2};
use crate::layer::GateType;
use crate::model::{MambaModel, MambaModelConfig};
use crate::Brain;
use crate::brain::now_f64;

/// Default maximum ring buffer length (history of brain state encodings).
const DEFAULT_MAX_HISTORY: usize = 256;

/// Number of top neuron activations sampled per encoding.
const TOP_K_NEURONS: usize = 20;

/// Features per module: activation_level, avg_importance, firing_rate, neuron_count.
const FEATURES_PER_MODULE: usize = 4;

/// Number of brain modules (fixed 11).
const NUM_MODULES: usize = 11;

/// SSM Cortex configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CortexConfig {
    /// SSM model dimension (also the encoding dimension)
    pub d_model: usize,
    /// SSM state dimension (N)
    pub state_dim: usize,
    /// Number of Mamba blocks
    pub n_layers: usize,
    /// Expansion factor
    pub expand_factor: usize,
    /// Convolution kernel size
    pub conv_kernel: usize,
    /// Whether the cortex is enabled
    pub enabled: bool,
    /// How strongly the cortex modulates the brain (0.0 = none, 1.0 = full)
    pub modulation_strength: f64,
    /// Run cortex step every N brain simulation ticks (0 = manual only)
    pub tick_interval: u64,
    /// Maximum history buffer length
    pub max_history: usize,
}

impl Default for CortexConfig {
    fn default() -> Self {
        Self {
            d_model: 16,
            state_dim: 4,
            n_layers: 2,
            expand_factor: 2,
            conv_kernel: 3,
            enabled: true,
            modulation_strength: 0.3,
            tick_interval: 10,
            max_history: DEFAULT_MAX_HISTORY,
        }
    }
}

/// Feature dimension for encoding matrices.
fn feature_dim() -> usize {
    NUM_MODULES * FEATURES_PER_MODULE + TOP_K_NEURONS
}

/// SSM Cortex — brain state sequence processor powered by MambaModel.
pub struct SsmCortex {
    /// Cortex configuration
    pub config: CortexConfig,
    /// MambaModel — provides stacked Mamba blocks + RMSNorm + output projection
    model: MambaModel,
    /// Input projection: brain features → model dimension (learned, not random)
    input_proj: Array2<f64>,
    /// Modulation decoder: model output → per-module modulation signals
    mod_proj: Array2<f64>,
    /// Next-state prediction projection: model output → feature dimension
    pred_proj: Array2<f64>,
    /// Ring buffer of past brain state embeddings (VecDeque for O(1) pop_front)
    state_buffer: VecDeque<Array1<f64>>,
    /// Compressed summary of older states
    summary_buffer: VecDeque<Array1<f64>>,
    /// Last raw encoding (before projection)
    last_raw_encoding: Vec<f64>,
    /// Last SSM output vector
    last_output: Array1<f64>,
    /// Cumulative tick counter
    tick: u64,
    /// ── Online learning state ──
    /// Learning rate
    hebbian_lr: f64,
    /// Running trace of feature correlations
    feature_trace: Vec<f64>,
    /// Last modulation signal per module
    last_mod_signals: Vec<f64>,
    /// How many cortex steps to wait between learning updates
    learn_interval: u64,
    ticks_since_learn: u64,
    /// Next-state prediction error (for monitoring)
    pub last_prediction_error: f64,
}

impl Default for SsmCortex {
    fn default() -> Self {
        Self::new(CortexConfig::default())
    }
}

impl std::fmt::Debug for SsmCortex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SsmCortex")
            .field("config", &self.config)
            .field("state_buffer_len", &self.state_buffer.len())
            .field("tick", &self.tick)
            .finish()
    }
}

/// A single cortex step result.
#[derive(Debug)]
pub struct StepOutput {
    /// Whether modulation was applied
    pub modulated: bool,
    /// Number of timesteps in history buffer
    pub history_len: usize,
    /// SSM output norm (L2)
    pub output_norm: f64,
    /// Feature vector before projection
    pub raw_features: usize,
}

impl SsmCortex {
    /// Create a new SSM cortex with MambaModel as the core processor.
    pub fn new(config: CortexConfig) -> Self {
        let d = config.d_model;
        let max_history = config.max_history;
        let feat_dim = feature_dim();

        // Create MambaModel with a minimal vocab — we bypass the token embedding
        // and feed continuous state vectors directly into the layers.
        let mm_config = MambaModelConfig {
            vocab_size: 1,          // not used — we feed features manually
            n_layers: config.n_layers,
            d_model: d,
            state_dim: config.state_dim,
            expand_factor: config.expand_factor,
            conv_kernel: config.conv_kernel,
            eps: 1e-5,
            weight_tied: false,
            gate_type: GateType::SiLU,
        };
        let model = MambaModel::new(mm_config);

        // Learned input projection: feat_dim → d_model
        let mut rng = rand::rng();
        let scale = (1.0 / d as f64).sqrt();
        let input_proj: Array2<f64> = Array2::zeros((d, feat_dim)).mapv(|_: f64| {
            rand::Rng::random_range(&mut rng, -scale..scale)
        });

        // Modulation decoder: d_model → num_modules
        let mod_proj: Array2<f64> = Array2::zeros((NUM_MODULES, d)).mapv(|_: f64| {
            rand::Rng::random_range(&mut rng, -0.1..0.1)
        });

        // Prediction projection: d_model → feat_dim (for next-state prediction learning)
        let pred_proj: Array2<f64> = Array2::zeros((feat_dim, d)).mapv(|_: f64| {
            rand::Rng::random_range(&mut rng, -scale..scale)
        });

        Self {
            config,
            model,
            input_proj,
            mod_proj,
            pred_proj,
            state_buffer: VecDeque::with_capacity(max_history),
            summary_buffer: VecDeque::with_capacity(max_history / 4),
            last_raw_encoding: Vec::new(),
            last_output: Array1::zeros(d),
            tick: 0,
            hebbian_lr: 0.001,
            feature_trace: vec![0.0; feat_dim],
            last_mod_signals: vec![0.0; NUM_MODULES],
            learn_interval: 5,
            ticks_since_learn: 0,
            last_prediction_error: 0.0,
        }
    }

    /// Encode the current brain state into a feature vector.
    ///
    /// Builds a descriptor from:
    /// - Per-module: activation_level, avg_importance, firing_rate, neuron_count (normalized)
    /// - Top-K most active neuron activations across the brain
    fn encode_brain(&self, brain: &Brain) -> Vec<f64> {
        let now = now_f64();
        let feat_dim = feature_dim();
        let module_names = &brain.module_order;
        let mut features = Vec::with_capacity(feat_dim);

        for name in module_names {
            if let Some(module) = brain.modules.get(name) {
                let indices = &module.neuron_indices;
                let count = indices.len();
                if count == 0 {
                    features.push(module.activation_level as f64);
                    features.push(module.importance as f64);
                    features.push(0.0);
                    features.push(0.0);
                    continue;
                }

                // Module activation level (already 0-1)
                let act = module.activation_level as f64;

                // Average importance across neurons in module
                let avg_imp: f64 = indices.iter()
                    .map(|&i| brain.neurons[i].importance as f64)
                    .sum::<f64>() / count as f64;

                // Firing rate: fraction that fired in the last 500ms
                let firing_rate: f64 = indices.iter()
                    .filter(|&&i| {
                        let ft = brain.neurons[i].spike_time;
                        ft > 0.0 && (now - ft) < 0.5
                    })
                    .count() as f64 / count as f64;

                // Normalized neuron count (relative to 3000)
                let norm_count = (count as f64 / 3000.0).min(1.0);

                features.push(act);
                features.push(avg_imp);
                features.push(firing_rate);
                features.push(norm_count);
            }
        }

        // Pad/truncate to exactly 11 modules
        let mod_feat_count = NUM_MODULES * FEATURES_PER_MODULE;
        while features.len() < mod_feat_count {
            features.push(0.0);
        }
        features.truncate(mod_feat_count);

        // Top-K neuron activations
        let mut neurons: Vec<(f32, &crate::Neuron)> = brain.neurons.iter()
            .map(|n| (n.activation, n))
            .collect();
        neurons.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        for (_, n) in neurons.iter().take(TOP_K_NEURONS) {
            features.push(n.activation as f64);
        }
        while features.len() < feat_dim {
            features.push(0.0);
        }

        features
    }

    /// Project brain features into model dimension via the learned input projection.
    fn project_to_embedding(&self, features: &[f64]) -> Array1<f64> {
        let d = self.config.d_model;
        let feat_len = features.len().min(self.input_proj.shape()[1]);
        let mut emb = Array1::<f64>::zeros(d);

        for i in 0..d {
            let mut sum: f64 = 0.0;
            for j in 0..feat_len {
                sum += self.input_proj[[i, j]] * features[j];
            }
            emb[i] = sum.tanh();
        }

        emb
    }

    /// Run the MambaModel's RMSNorm on a sequence matrix.
    ///
    /// Duplicates MambaModel::rms_norm since that method is private,
    /// using the model's norm_scale parameter.
    fn rms_norm_seq(&self, x: &Array2<f64>) -> Array2<f64> {
        let (rows, cols) = (x.shape()[0], x.shape()[1]);
        let eps = self.model.config.eps;
        let mut out = Array2::zeros((rows, cols));

        for i in 0..rows {
            let row = x.row(i);
            let mean_sq: f64 = row.iter().map(|v| v * v).sum::<f64>() / cols as f64;
            let rms = (mean_sq + eps).sqrt();
            for j in 0..cols {
                out[[i, j]] = (row[j] / rms) * self.model.norm_scale[j];
            }
        }

        out
    }

    /// Run the forward pass through MambaModel's layers + RMSNorm.
    ///
    /// Input: (L, d_model) sequence of state embeddings.
    /// Output: processed (L, d_model) sequence.
    fn mamba_forward(&self, seq: &Array2<f64>) -> Array2<f64> {
        let mut h = seq.clone();
        for layer in &self.model.layers {
            h = layer.forward(&h);
        }
        self.rms_norm_seq(&h)
    }

    /// Decode MambaModel output into per-module modulation signals.
    fn decode_modulation(&self, ssm_output: &Array1<f64>) -> Vec<f64> {
        let num_modules = self.mod_proj.shape()[0];
        let mut signals = Vec::with_capacity(num_modules);
        for i in 0..num_modules {
            let mut signal = 0.0;
            for j in 0..ssm_output.len() {
                signal += self.mod_proj[[i, j]] * ssm_output[j];
            }
            signals.push(signal.tanh());
        }
        signals
    }

    /// Predict the next brain state from current SSM output.
    /// Returns predicted feature vector (feat_dim,).
    fn predict_next_state(&self, ssm_output: &Array1<f64>) -> Array1<f64> {
        let feat_dim = self.pred_proj.shape()[0];
        let mut pred = Array1::zeros(feat_dim);
        for i in 0..feat_dim {
            let mut sum = 0.0;
            for j in 0..ssm_output.len() {
                sum += self.pred_proj[[i, j]] * ssm_output[j];
            }
            pred[i] = sum;
        }
        pred
    }

    /// Run one step of the SSM cortex:
    /// 1. Encode brain state
    /// 2. Project to embedding, push to ring buffer
    /// 3. Run MambaModel over the sequence
    /// 4. Decode output into modulation signals
    /// 5. Apply modulation to brain
    /// 6. Online learning (Hebbian + prediction error)
    pub fn step(&mut self, brain: &mut Brain) -> StepOutput {
        let d = self.config.d_model;

        // 1. Encode
        let raw = self.encode_brain(brain);
        let embedding = self.project_to_embedding(&raw);
        self.last_raw_encoding = raw;

        // 2. Push to ring buffer with summarization
        let max = self.config.max_history;
        self.state_buffer.push_back(embedding);
        if self.state_buffer.len() > max {
            let chunk_size = (max / 4).max(4);
            let mut sum = Array1::<f64>::zeros(d);
            let mut count = 0;
            while let Some(old) = self.state_buffer.pop_front() {
                sum = sum + &old;
                count += 1;
                if count >= chunk_size {
                    break;
                }
            }
            if count > 0 {
                let avg = sum.mapv(|v| v / count as f64);
                self.summary_buffer.push_back(avg);
                if self.summary_buffer.len() > max / 4 {
                    self.summary_buffer.pop_front();
                }
            }
        }

        // 3. Build sequence and run through MambaModel
        let summary_len = self.summary_buffer.len();
        let buf_len = self.state_buffer.len();
        let total_len = summary_len + buf_len;
        let mut seq = Array2::zeros((total_len, d));

        for (t, vec) in self.summary_buffer.iter().enumerate() {
            for j in 0..d {
                seq[[t, j]] = vec[j];
            }
        }
        for (t, vec) in self.state_buffer.iter().enumerate() {
            for j in 0..d {
                seq[[summary_len + t, j]] = vec[j];
            }
        }

        // Process through MambaModel (stacked blocks + RMSNorm)
        let h = self.mamba_forward(&seq);

        // Extract last output vector
        let last_idx = h.shape()[0] - 1;
        let mut output = Array1::zeros(d);
        for j in 0..d {
            output[j] = h[[last_idx, j]];
        }
        self.last_output = output.clone();

        // 4. Decode into modulation signals
        let output_norm = output.iter().map(|v| v * v).sum::<f64>().sqrt();
        let mut modulated = false;

        if self.config.enabled {
            modulated = self.modulate(brain, &output);
        }

        // 5. Online Hebbian learning + prediction error
        self.ticks_since_learn += 1;
        if self.config.enabled && self.ticks_since_learn >= self.learn_interval {
            self.online_learn(brain, &output);
            self.ticks_since_learn = 0;
        }

        self.tick += 1;

        StepOutput {
            modulated,
            history_len: total_len,
            output_norm,
            raw_features: self.last_raw_encoding.len(),
        }
    }

    /// Apply SSM output modulation to the brain.
    fn modulate(&self, brain: &mut Brain, ssm_output: &Array1<f64>) -> bool {
        let strength = self.config.modulation_strength;
        if strength < 0.001 {
            return false;
        }

        let signals = self.decode_modulation(ssm_output);
        let num_modules = brain.module_order.len().min(signals.len());
        let mut any_modulated = false;

        for (i, name) in brain.module_order.iter().enumerate() {
            if i >= num_modules {
                break;
            }

            let signal = signals[i] * strength;
            if signal.abs() < 0.001 {
                continue;
            }
            any_modulated = true;

            // Apply to module's activation level
            if let Some(module) = brain.modules.get_mut(name) {
                module.activation_level = (module.activation_level + signal as f32)
                    .clamp(0.0, 1.0);
                module.importance = (module.importance + (signal * 0.3) as f32)
                    .clamp(0.0, 1.0);
            }

            // Boost/attenuate neuron potentials
            if let Some(module) = brain.modules.get(name) {
                let indices = module.neuron_indices.clone();
                let boost = (signal * 0.05) as f32;
                for &ni in &indices {
                    if let Some(neuron) = brain.neurons.get_mut(ni) {
                        neuron.potential = (neuron.potential + boost).clamp(0.0, 1.0);
                        if signal.abs() > 0.3 {
                            neuron.importance = (neuron.importance + boost.abs() * 0.1)
                                .min(1.0);
                        }
                    }
                }
            }
        }

        any_modulated
    }

    /// Online Hebbian learning for input/mod/pred projections.
    ///
    /// Three update signals:
    /// 1. Input projection: Oja's rule on feature → embedding correlation
    /// 2. Modulation projection: reward-prediction error on module activation
    /// 3. Prediction projection: MSE on next-state prediction
    fn online_learn(&mut self, brain: &Brain, ssm_output: &Array1<f64>) {
        let d = self.config.d_model;
        let feat_len = self.input_proj.shape()[1];
        let lr = self.hebbian_lr;

        // ── 1. Update input projection (Oja's rule) ──
        let raw_features = &self.last_raw_encoding;
        if raw_features.len() >= feat_len {
            for i in 0..d {
                for j in 0..feat_len {
                    let cur = self.input_proj[[i, j]];
                    let oja = lr * ssm_output[i] * (raw_features[j] - cur * ssm_output[i].abs());
                    self.input_proj[[i, j]] = (cur + oja).clamp(-5.0, 5.0);
                    self.feature_trace[j] = self.feature_trace[j] * 0.95 + raw_features[j].abs() * 0.05;
                }
            }
        }

        // ── 2. Update modulation projection (reward-prediction) ──
        if !self.last_mod_signals.is_empty() {
            let num_mods = brain.module_order.len().min(self.mod_proj.shape()[0]);
            for i in 0..num_mods {
                if let Some(module) = brain.modules.get(&brain.module_order[i]) {
                    let actual_act = module.activation_level as f64;
                    let predicted = self.last_mod_signals[i];
                    let error = actual_act - predicted.tanh();
                    for j in 0..d {
                        let cur = self.mod_proj[[i, j]];
                        let delta = lr * 0.5 * error * ssm_output[j];
                        self.mod_proj[[i, j]] = (cur + delta).clamp(-3.0, 3.0);
                    }
                }
            }
        }

        // ── 3. Update prediction projection (next-state prediction) ──
        // The SSM output should predict the NEXT feature vector.
        // Since we just encoded the current state, the next cortex step will
        // produce the next feature vector. We use the current features as
        // the target (predicting the next tick's state from accumulated history).
        if feat_len == raw_features.len() {
            let predicted = self.predict_next_state(ssm_output);
            let mut mse = 0.0;
            for i in 0..feat_len.min(predicted.len()) {
                let target = raw_features[i];
                let pred = predicted[i];
                let error = target - pred;
                mse += error * error;
                // Update prediction projection weights
                for j in 0..d.min(ssm_output.len()) {
                    let cur = self.pred_proj[[i, j]];
                    let delta = lr * 0.3 * error * ssm_output[j];
                    self.pred_proj[[i, j]] = (cur + delta).clamp(-5.0, 5.0);
                }
            }
            self.last_prediction_error = (mse / feat_len.max(1) as f64).sqrt();
        }

        // Store current modulation signals for next learning step
        self.last_mod_signals.clear();
        for name in &brain.module_order {
            if let Some(module) = brain.modules.get(name) {
                self.last_mod_signals.push(module.activation_level as f64);
            } else {
                self.last_mod_signals.push(0.0);
            }
        }
    }

    /// Process a single brain state without updating history or modulating.
    /// Pure forward pass through the MambaModel for analysis.
    pub fn forward(&self, brain: &Brain) -> Array1<f64> {
        let raw = self.encode_brain(brain);
        let embedding = self.project_to_embedding(&raw);

        let d = self.config.d_model;
        let mut seq = Array2::zeros((1, d));
        for j in 0..d {
            seq[[0, j]] = embedding[j];
        }

        let h = self.mamba_forward(&seq);

        let mut output = Array1::zeros(d);
        for j in 0..d {
            output[j] = h[[0, j]];
        }
        output
    }

    /// Get cortex status as a JSON value for the API.
    pub fn api_status(&self) -> serde_json::Value {
        let output_norm: f64 = self.last_output.iter()
            .map(|v| v * v).sum::<f64>().sqrt();

        let last_output_vec: Vec<f64> = self.last_output.iter()
            .take(8)
            .copied()
            .collect();

        serde_json::json!({
            "enabled": self.config.enabled,
            "d_model": self.config.d_model,
            "state_dim": self.config.state_dim,
            "n_layers": self.config.n_layers,
            "modulation_strength": self.config.modulation_strength,
            "tick_interval": self.config.tick_interval,
            "history_len": self.state_buffer.len(),
            "max_history": self.config.max_history,
            "tick": self.tick,
            "last_output_norm": (output_norm * 1000.0).round() / 1000.0,
            "last_output": last_output_vec,
            "params_estimate": self.param_count(),
            "blocks": self.model.layers.len(),
            "prediction_error": (self.last_prediction_error * 1000.0).round() / 1000.0,
        })
    }

    /// Rough parameter count for status display.
    fn param_count(&self) -> usize {
        let d = self.config.d_model;
        let d_inner = d * self.config.expand_factor;
        let s = self.config.state_dim;
        let k = self.config.conv_kernel;

        let per_block = 2 * d_inner * d       // in_proj
            + d_inner * k                     // conv_weight
            + d_inner                         // conv_bias
            + s * s + 2 * s * d_inner + d_inner // SSM
            + d * d_inner                     // out_proj
            + 2 * d;                          // norm

        let feat_dim = feature_dim();
        let input_params = d * feat_dim;
        let mod_params = NUM_MODULES * d;
        let pred_params = feat_dim * d;
        let output_proj_params = self.model.output_proj.len(); // from MambaModel

        self.model.layers.len() * per_block
            + input_params + mod_params + pred_params
            + output_proj_params
            + d // norm_scale
    }

    /// Enable or disable the cortex.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.config.enabled = enabled;
    }

    /// Toggle enabled state.
    pub fn toggle(&mut self) -> bool {
        self.config.enabled = !self.config.enabled;
        self.config.enabled
    }

    /// Update configuration parameters.
    pub fn configure(&mut self, modulation_strength: Option<f64>, tick_interval: Option<u64>, enabled: Option<bool>) {
        if let Some(s) = modulation_strength {
            self.config.modulation_strength = s.clamp(0.0, 1.0);
        }
        if let Some(t) = tick_interval {
            self.config.tick_interval = t;
        }
        if let Some(e) = enabled {
            self.config.enabled = e;
        }
    }
}


// ── Cortex persistence ──

/// Serializable snapshot of the cortex state (projections + ring buffer).
#[derive(Clone, Serialize, Deserialize)]
pub struct CortexSnapshot {
    pub config: CortexConfig,
    pub input_proj: Vec<Vec<f64>>,
    pub input_proj_shape: (usize, usize),
    pub mod_proj: Vec<Vec<f64>>,
    pub mod_proj_shape: (usize, usize),
    pub pred_proj: Vec<Vec<f64>>,
    pub pred_proj_shape: (usize, usize),
    pub state_buffer: Vec<Vec<f64>>,
    pub summary_buffer: Vec<Vec<f64>>,
    pub last_raw_encoding: Vec<f64>,
    pub last_output: Vec<f64>,
    pub tick: u64,
    pub feature_trace: Vec<f64>,
    pub last_mod_signals: Vec<f64>,
    pub last_prediction_error: f64,
}

impl SsmCortex {
    /// Export a serializable snapshot of the current cortex state.
    pub fn snapshot(&self) -> CortexSnapshot {
        let to_vec2 = |a: &Array2<f64>| -> Vec<Vec<f64>> {
            let shape = a.shape();
            let mut v = Vec::with_capacity(shape[0]);
            for i in 0..shape[0] {
                let mut row = Vec::with_capacity(shape[1]);
                for j in 0..shape[1] {
                    row.push(a[[i, j]]);
                }
                v.push(row);
            }
            v
        };
        let to_vec1 = |a: &Array1<f64>| -> Vec<f64> {
            a.iter().copied().collect()
        };
        let buf_to_vec = |buf: &VecDeque<Array1<f64>>| -> Vec<Vec<f64>> {
            buf.iter().map(|a| a.iter().copied().collect()).collect()
        };

        CortexSnapshot {
            config: self.config.clone(),
            input_proj: to_vec2(&self.input_proj),
            input_proj_shape: self.input_proj.dim(),
            mod_proj: to_vec2(&self.mod_proj),
            mod_proj_shape: self.mod_proj.dim(),
            pred_proj: to_vec2(&self.pred_proj),
            pred_proj_shape: self.pred_proj.dim(),
            state_buffer: buf_to_vec(&self.state_buffer),
            summary_buffer: buf_to_vec(&self.summary_buffer),
            last_raw_encoding: self.last_raw_encoding.clone(),
            last_output: to_vec1(&self.last_output),
            tick: self.tick,
            feature_trace: self.feature_trace.clone(),
            last_mod_signals: self.last_mod_signals.clone(),
            last_prediction_error: self.last_prediction_error,
        }
    }

    /// Restore cortex state from a snapshot.
    pub fn load_snapshot(&mut self, snap: &CortexSnapshot) {
        let from_vec2 = |v: &[Vec<f64>], shape: (usize, usize)| -> Array2<f64> {
            let mut a = Array2::zeros((shape.0, shape.1));
            for i in 0..shape.0.min(v.len()) {
                for j in 0..shape.1.min(v[i].len()) {
                    a[[i, j]] = v[i][j];
                }
            }
            a
        };
        let from_vec1 = |v: &[f64]| -> Array1<f64> {
            Array1::from_vec(v.to_vec())
        };
        let from_vec_buf = |v: &[Vec<f64>]| -> VecDeque<Array1<f64>> {
            v.iter().map(|row| Array1::from_vec(row.clone())).collect()
        };

        self.config = snap.config.clone();
        self.input_proj = from_vec2(&snap.input_proj, snap.input_proj_shape);
        self.mod_proj = from_vec2(&snap.mod_proj, snap.mod_proj_shape);
        self.pred_proj = from_vec2(&snap.pred_proj, snap.pred_proj_shape);
        self.state_buffer = from_vec_buf(&snap.state_buffer);
        self.summary_buffer = from_vec_buf(&snap.summary_buffer);
        self.last_raw_encoding = snap.last_raw_encoding.clone();
        self.last_output = from_vec1(&snap.last_output);
        self.tick = snap.tick;
        self.feature_trace = snap.feature_trace.clone();
        self.last_mod_signals = snap.last_mod_signals.clone();
        self.last_prediction_error = snap.last_prediction_error;
    }
}

/// Save the cortex state snapshot to a JSON file.
pub fn save_cortex_state(path: &str, cortex: &SsmCortex) -> Result<(), Box<dyn std::error::Error>> {
    let snap = cortex.snapshot();
    let json = serde_json::to_string_pretty(&snap)?;
    std::fs::write(path, json)?;
    Ok(())
}

/// Load the cortex state snapshot from a JSON file and apply it.
pub fn load_cortex_state(path: &str, cortex: &mut SsmCortex) -> Result<(), Box<dyn std::error::Error>> {
    let json = std::fs::read_to_string(path)?;
    let snap: CortexSnapshot = serde_json::from_str(&json)?;
    cortex.load_snapshot(&snap);
    Ok(())
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cortex_new() {
        let config = CortexConfig::default();
        let cortex = SsmCortex::new(config);
        assert_eq!(cortex.model.layers.len(), 2);
        assert!(cortex.config.enabled);
    }

    #[test]
    fn test_encode_brain_returns_correct_size() {
        let brain = Brain::new("test", 9999);
        let raw = {
            let cortex = SsmCortex::new(CortexConfig::default());
            cortex.encode_brain(&brain)
        };
        let expected = 11 * FEATURES_PER_MODULE + TOP_K_NEURONS;
        assert_eq!(raw.len(), expected, "Feature vector should have {} dims", expected);
    }

    #[test]
    fn test_step_returns_output() {
        let mut brain = Brain::new("test", 9999);
        let mut cortex = SsmCortex::new(CortexConfig::default());

        let output = cortex.step(&mut brain);
        assert_eq!(output.history_len, 1);
        assert!(output.raw_features > 0);
    }

    #[test]
    fn test_step_builds_history() {
        let mut brain = Brain::new("test", 9999);
        let mut cortex = SsmCortex::new(CortexConfig::default());

        for _ in 0..5 {
            cortex.step(&mut brain);
        }
        assert_eq!(cortex.state_buffer.len(), 5);

        let max = cortex.config.max_history;
        for _ in 0..max + 20 {
            cortex.step(&mut brain);
        }
        assert!(cortex.state_buffer.len() <= max, "buf {} > max {}", cortex.state_buffer.len(), max);
        assert!(cortex.summary_buffer.len() > 0, "summary buffer should have compressed states");
    }

    #[test]
    fn test_forward_returns_vector() {
        let brain = Brain::new("test", 9999);
        let cortex = SsmCortex::new(CortexConfig::default());

        let output = cortex.forward(&brain);
        assert_eq!(output.len(), 16);
    }

    #[test]
    fn test_toggle() {
        let mut cortex = SsmCortex::new(CortexConfig::default());
        assert!(cortex.config.enabled);

        cortex.toggle();
        assert!(!cortex.config.enabled);

        cortex.toggle();
        assert!(cortex.config.enabled);
    }

    #[test]
    fn test_configure() {
        let mut cortex = SsmCortex::new(CortexConfig::default());

        cortex.configure(Some(0.8), Some(5), Some(false));
        assert!((cortex.config.modulation_strength - 0.8).abs() < 1e-6);
        assert_eq!(cortex.config.tick_interval, 5);
        assert!(!cortex.config.enabled);

        cortex.configure(Some(2.0), None, None);
        assert!((cortex.config.modulation_strength - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_api_status_shape() {
        let mut brain = Brain::new("test", 9999);
        let mut cortex = SsmCortex::new(CortexConfig::default());
        cortex.step(&mut brain);

        let status = cortex.api_status();
        assert!(status.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false));
        assert!(status.get("d_model").and_then(|v| v.as_u64()).unwrap_or(0) == 16);
        assert!(status.get("history_len").and_then(|v| v.as_u64()).unwrap_or(0) >= 1);
        assert!(status.get("last_output").is_some());
        assert!(status.get("prediction_error").is_some());
    }

    #[test]
    fn test_decode_modulation() {
        let cortex = SsmCortex::new(CortexConfig::default());
        let d = cortex.config.d_model;
        let output = Array1::zeros(d);
        let signals = cortex.decode_modulation(&output);
        assert_eq!(signals.len(), NUM_MODULES);
    }

    #[test]
    fn test_predict_next_state() {
        let cortex = SsmCortex::new(CortexConfig::default());
        let d = cortex.config.d_model;
        let output = Array1::zeros(d);
        let prediction = cortex.predict_next_state(&output);
        assert_eq!(prediction.len(), feature_dim());
    }

    #[test]
    fn test_rms_norm_seq_maintains_shape() {
        let cortex = SsmCortex::new(CortexConfig::default());
        let d = cortex.config.d_model;
        // Use non-zero random-ish values so variance is non-zero
        let mut data = Array2::zeros((5, d));
        for i in 0..5 {
            for j in 0..d {
                data[[i, j]] = 0.5 + (i as f64 * 0.1) + (j as f64 * 0.01);
            }
        }
        let normalized = cortex.rms_norm_seq(&data);
        assert_eq!(normalized.shape(), &[5, d]);
        // Check RMS of each row is approximately 1.0
        for i in 0..5 {
            let row = normalized.row(i);
            let mean_sq = row.iter().map(|v| v * v).sum::<f64>() / d as f64;
            assert!((mean_sq - 1.0).abs() < 0.001, "RMS norm should produce unit variance, got {}", mean_sq);
        }
    }

    #[test]
    fn test_mamba_forward_maintains_shape() {
        let cortex = SsmCortex::new(CortexConfig::default());
        let d = cortex.config.d_model;
        let seq = Array2::zeros((3, d));
        let output = cortex.mamba_forward(&seq);
        assert_eq!(output.shape(), &[3, d]);
    }

    #[test]
    fn test_online_learn_no_panic() {
        let mut brain = Brain::new("test", 9999);
        let mut cortex = SsmCortex::new(CortexConfig::default());

        // Run a few steps to build state
        for _ in 0..10 {
            cortex.step(&mut brain);
        }

        // online_learn is called inside step(), so it should have run
        assert!(cortex.last_prediction_error >= 0.0);
    }
}

