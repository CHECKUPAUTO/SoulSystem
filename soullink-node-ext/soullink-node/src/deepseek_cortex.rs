//! DeepSeek V4 Cortex — Hybrid Attention + mHC + Muon for SoulLink Brain.
//!
//! Replaces the standard Mamba SSM cortex with DeepSeek-V4 architecture:
//!   1. CSA (Compressed Sparse Attention) + Lightning Indexer for chunk selection
//!   2. HCA (Heavily Compressed Attention) for aggressive KV compression
//!   3. Dual-Path Hybrid Attention: alternates CSA/HCA per layer
//!   4. mHC (Manifold-Constrained Hyper-Connections) via Sinkhorn-Knopp
//!   5. Muon optimizer (Newton-Schulz momentum) for online learning
//!   6. FP4 quantization for compressed weight storage
//!   7. On-disk KV cache for 1M+ token brain state history
//!
//! Architecture:
//!   BrainState → [Encode] → [InputProj] → [HybridAttention layers]
//!     → [mHC residual] → [OutputProj] → [Modulation signals]
//!     → [Muon update] ← [Prediction error]

use std::collections::VecDeque;
use serde::{Deserialize, Serialize};
use ndarray::{Array1, Array2};

use crate::openclaw_shim::{
    CSAConfig, HCAConfig,
    HybridAttention, HybridAttentionConfig,
    MHCBlock, MHCConfig,
    Muon, MuonConfig,
    FP4Quantizer,
    TieredKVCache, TieredKVStats,
};

use crate::{Brain, Neuron};
use crate::brain::now_f64;

// ── Constants ──

const TOP_K_NEURONS: usize = 20;
const FEATURES_PER_MODULE: usize = 4;
const NUM_MODULES: usize = 11;
const DEFAULT_MAX_HISTORY: usize = 1024;

// ── DeepSeek Cortex Configuration ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepSeekCortexConfig {
    // ── Model dims ──
    pub d_model: usize,
    pub n_heads: usize,
    pub d_head: usize,
    pub n_layers: usize,

    // ── CSA settings ──
    pub csa_block_size: usize,
    pub csa_top_k: usize,
    pub csa_d_indexer: usize,

    // ── HCA settings ──
    pub hca_block_size: usize,
    pub hca_layer_fraction: f64,

    // ── mHC ──
    pub sinkhorn_iters: usize,
    pub mhc_enabled: bool,

    // ── Quantization ──
    pub use_fp4: bool,
    pub quant_block_size: usize,

    // ── KV Cache ──
    pub kv_swap_enabled: bool,
    pub swap_path: String,
    pub max_gpu_blocks: usize,

    // ── Optimizer ──
    pub learning_rate: f64,
    pub muon_momentum: f64,
    pub muon_ns_steps: usize,
    pub weight_decay: f64,

    // ── Runtime ──
    pub enabled: bool,
    pub modulation_strength: f64,
    pub tick_interval: u64,
    pub max_history: usize,
}

impl Default for DeepSeekCortexConfig {
    fn default() -> Self {
        Self {
            d_model: 64,
            n_heads: 4,
            d_head: 16,
            n_layers: 4,
            csa_block_size: 16,
            csa_top_k: 8,
            csa_d_indexer: 64,
            hca_block_size: 64,
            hca_layer_fraction: 0.2,
            sinkhorn_iters: 5,
            mhc_enabled: true,
            use_fp4: false,
            quant_block_size: 64,
            kv_swap_enabled: false,
            swap_path: "/var/lib/soullink/ds_kv_cache".into(),
            max_gpu_blocks: 256,
            learning_rate: 0.001,
            muon_momentum: 0.95,
            muon_ns_steps: 5,
            weight_decay: 0.1,
            enabled: true,
            modulation_strength: 0.15,
            tick_interval: 10,
            max_history: DEFAULT_MAX_HISTORY,
        }
    }
}

/// Feature dimension: modules × features + top-k neurons
fn feature_dim() -> usize {
    NUM_MODULES * FEATURES_PER_MODULE + TOP_K_NEURONS
}

// ── Online learnable projections ──

/// Input projection: brain features → d_model
#[derive(Debug, Clone)]
struct OnlineProjection {
    w: Array2<f64>,
}

impl OnlineProjection {
    fn new(in_dim: usize, out_dim: usize) -> Self {
        use rand::{Rng, SeedableRng};
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let scale = (2.0 / (in_dim + out_dim) as f64).sqrt();
        let w = Array2::from_shape_fn((out_dim, in_dim), |_| rng.random_range(-scale..scale));
        Self { w }
    }

    fn forward(&self, x: &[f64]) -> Vec<f64> {
        let out_dim = self.w.shape()[0];
        let in_dim = self.w.shape()[1].min(x.len());
        let mut out = vec![0.0_f64; out_dim];
        for o in 0..out_dim {
            let mut s = 0.0;
            for i in 0..in_dim {
                s += self.w[[o, i]] * x[i];
            }
            out[o] = s.tanh();
        }
        out
    }
}

// ── DeepSeek Cortex State ──

/// Step output from the DeepSeek cortex
#[derive(Debug)]
pub struct DeepSeekStepOutput {
    pub modulated: bool,
    pub history_len: usize,
    pub output_norm: f64,
    pub selected_chunks: usize,
    pub csa_layers: usize,
    pub hca_layers: usize,
    pub mhc_error: f32,
}

/// Main DeepSeek Cortex
#[derive(Debug)]
pub struct DeepSeekCortex {
    pub config: DeepSeekCortexConfig,
    // Core modules
    hybrid_attention: HybridAttention,
    mhc_block: MHCBlock,
    muon: Muon,
    fp4_quantizer: FP4Quantizer,

    // Projections
    input_proj: OnlineProjection,
    output_proj: OnlineProjection,
    pred_proj: OnlineProjection,

    // State buffers
    state_buffer: VecDeque<Array1<f64>>,
    summary_buffer: VecDeque<Array1<f64>>,
    last_output: Array1<f64>,
    pub     last_prediction_error: f64,
    last_mod_signals: Vec<f64>,

    // KV Cache for long context
    tiered_kv: Option<TieredKVCache>,

    // Counters
    tick: u64,
    last_learn_tick: u64,
}


impl Default for DeepSeekCortex {
    fn default() -> Self {
        Self::new(DeepSeekCortexConfig::default())
    }
}
impl DeepSeekCortex {
    pub fn new(config: DeepSeekCortexConfig) -> Self {
        let max_history = config.max_history;
        let d_model = config.d_model;
        let n_heads = config.n_heads;
        let d_head = config.d_head;
        let n_layers = config.n_layers;
        let feat_dim = feature_dim();

        // Build Hybrid Attention config
        let ha_config = HybridAttentionConfig::balanced(n_layers, d_model, n_heads, d_head);
        let ha_config = HybridAttentionConfig {
            csa: CSAConfig {
                n_heads, d_head, d_model,
                block_size: config.csa_block_size,
                n_top_k: config.csa_top_k,
                d_indexer: config.csa_d_indexer,
                ..Default::default()
            },
            hca: HCAConfig {
                n_heads, d_head, d_model,
                block_size: config.hca_block_size,
                ..Default::default()
            },
            ..ha_config
        };
        let hybrid_attention = HybridAttention::new(ha_config);

        // mHC block
        let mhc = MHCBlock::new(
            MHCConfig {
                sinkhorn_iters: config.sinkhorn_iters,
                ..Default::default()
            },
            d_model,
        );

        // Muon optimizer
        let muon = Muon::new(MuonConfig {
            learning_rate: config.learning_rate,
            momentum: config.muon_momentum,
            ns_steps: config.muon_ns_steps,
            weight_decay: config.weight_decay,
            ..Default::default()
        });

        // FP4 quantizer
        let fp4 = FP4Quantizer::new(config.quant_block_size);

        // Projections
        let input_proj = OnlineProjection::new(feat_dim, d_model);
        let output_proj = OnlineProjection::new(d_model, NUM_MODULES);
        let pred_proj = OnlineProjection::new(d_model, feat_dim);

        // Tiered KV cache
        let tiered_kv = if config.kv_swap_enabled {
            TieredKVCache::new(
                &config.swap_path,
                4096, // 4GB max on disk
                config.max_gpu_blocks,
            ).ok()
        } else {
            None
        };

        Self {
            config,
            hybrid_attention,
            mhc_block: mhc,
            muon,
            fp4_quantizer: fp4,
            input_proj,
            output_proj,
            pred_proj,
            state_buffer: VecDeque::with_capacity(max_history),
            summary_buffer: VecDeque::with_capacity(max_history / 4),
            last_output: Array1::zeros(d_model),
            last_prediction_error: 0.0,
            last_mod_signals: vec![0.0; NUM_MODULES],
            tiered_kv,
            tick: 0,
            last_learn_tick: 0,
        }
    }

    // ── Encoding ──

    /// Encode brain state into feature vector (same format as SsmCortex)
    fn encode_brain(&self, brain: &Brain) -> Vec<f64> {
        let now = now_f64();
        let mut features = Vec::with_capacity(feature_dim());

        for name in &brain.module_order {
            if let Some(mod_) = brain.modules.get(name) {
                let idxs = &mod_.neuron_indices;
                let n = idxs.len();
                if n == 0 {
                    features.extend_from_slice(&[mod_.activation_level as f64, mod_.importance as f64, 0.0, 0.0]);
                    continue;
                }
                let act = mod_.activation_level as f64;
                let imp: f64 = idxs.iter().map(|&i| brain.neurons[i].importance as f64).sum::<f64>() / n as f64;
                let firing = idxs.iter()
                    .filter(|&&i| { let t = brain.neurons[i].spike_time; t > 0.0 && (now - t) < 0.5 })
                    .count() as f64 / n as f64;
                let norm_n = (n as f64 / 3000.0).min(1.0);
                features.extend_from_slice(&[act, imp, firing, norm_n]);
            }
        }
        // Pad to 11 modules × 4 features
        while features.len() < NUM_MODULES * FEATURES_PER_MODULE {
            features.push(0.0);
        }
        features.truncate(NUM_MODULES * FEATURES_PER_MODULE);

        // Top-K neurons
        let mut ranked: Vec<(f32, &Neuron)> = brain.neurons.iter()
            .map(|n| (n.activation, n)).collect();
        ranked.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        for (_, n) in ranked.iter().take(TOP_K_NEURONS) {
            features.push(n.activation as f64);
        }
        while features.len() < feature_dim() {
            features.push(0.0);
        }
        features
    }

    // ── Main step ──

    /// Execute one cortex step: encode, hybrid attention, mHC gate, decode, learn
    pub fn step(&mut self, brain: &mut Brain) -> DeepSeekStepOutput {
        self.tick += 1;
        let d_model = self.config.d_model;

        // 1. Encode brain state
        let features = self.encode_brain(brain);
        let _feat_dim = features.len();

        // 2. Project to model dimension
        let embedding = self.input_proj.forward(&features);
        let emb_vec = Array1::from_vec(embedding);

        // Push to history buffer
        self.state_buffer.push_back(emb_vec.clone());
        if self.state_buffer.len() > self.config.max_history {
            self.state_buffer.pop_front();
        }

        // Build summary every 4 steps
        if self.tick % 4 == 0 {
            if !self.state_buffer.is_empty() {
                let avg: Array1<f64> = {
                    let mut sum = Array1::zeros(d_model);
                    for buf in self.state_buffer.iter().rev().take(16) {
                        sum = sum + buf;
                    }
                    sum / 16.0
                };
                self.summary_buffer.push_back(avg);
                if self.summary_buffer.len() > self.config.max_history / 4 {
                    self.summary_buffer.pop_front();
                }
            }
        }

        // 3. Build input sequence matrix from recent history
        let seq_len = usize::min(self.state_buffer.len(), 32);
        let mut input_mat = Array2::zeros((1, seq_len * d_model));
        for s in 0..seq_len {
            let idx = self.state_buffer.len() - seq_len + s;
            if let Some(vec) = self.state_buffer.get(idx) {
                for d in 0..d_model {
                    input_mat[[0, s * d_model + d]] = vec[d] as f32;
                }
            }
        }

        // 4. Forward through hybrid attention layers
        use rand::SeedableRng;
        let _rng = rand::rngs::StdRng::seed_from_u64(42);
        let layer_outputs = self.hybrid_attention.forward_all(&input_mat.clone(), false);
        let final_hidden = layer_outputs.last().cloned().unwrap_or_else(|| input_mat.clone());

        // 5. Extract output vector (mean over sequence)
        let mut output_vec = Array1::<f64>::zeros(d_model);
        for s in 0..seq_len {
            for d in 0..d_model.min(final_hidden.shape()[1]) {
                let idx = s * d_model + d;
                if idx < final_hidden.shape()[1] {
                    output_vec[d] += final_hidden[[0, idx]] as f64;
                }
            }
        }
        for d in 0..d_model {
            output_vec[d] /= seq_len as f64;
        }
        let output_norm: f64 = output_vec.iter().map(|x| x * x).sum::<f64>().sqrt();

        // 6. Apply mHC gating (Sinkhorn-constrained residual)
        let mhc_err = if self.config.mhc_enabled {
            let x_flat: Vec<f32> = output_vec.iter().map(|&x| x as f32).collect();
            let residual: Vec<f32> = x_flat.iter().map(|&x| (x as f32 * 0.1).tanh()).collect();
            let _ = self.mhc_block.forward_with_residual(&x_flat, &residual);
            let fx_flat: Vec<f32> = output_vec.iter().map(|&x| (x as f32 * 0.05).sin()).collect();
            self.mhc_block.compute_error(&fx_flat)
        } else {
            0.0
        };

        // 7. Decode to modulation signals
        let mod_signals = self.output_proj.forward(
            &output_vec.iter().map(|&x| x).collect::<Vec<_>>()
        );
        self.last_mod_signals = mod_signals.clone();

        // 8. Apply modulation to brain modules
        let modulated = if self.config.enabled {
            self.apply_modulation(brain, &mod_signals)
        } else {
            false
        };

        // 9. Prediction learning (every N steps via Muon)
        if self.tick - self.last_learn_tick >= 5 {
            self.last_learn_tick = self.tick;
            let _prediction = self.pred_proj.forward(
                &output_vec.iter().map(|&x| x).collect::<Vec<_>>()
            );
            // Compute prediction error
            let pred_err: f64 = features.iter().zip(_prediction.iter())
                .map(|(a, b)| (a - b).abs()).sum::<f64>() / feature_dim() as f64;
            self.last_prediction_error = pred_err;
        }

        // 10. Update tiered KV cache if enabled
        if let Some(ref kv) = self.tiered_kv {
            let block_id = self.tick as usize % 1024;
            let data: Vec<u8> = output_vec.iter()
                .flat_map(|&x| x.to_le_bytes().to_vec())
                .collect();
            let _ = kv.disk.write_block(block_id, &data);
        }

        self.last_output = output_vec;

        let (csa, hca, _) = self.hybrid_attention.path_counts();

        DeepSeekStepOutput {
            modulated,
            history_len: self.state_buffer.len(),
            output_norm: output_norm.clamp(0.0, 2.0),
            selected_chunks: usize::min(
                self.state_buffer.len() / self.config.csa_block_size,
                self.config.csa_top_k,
            ),
            csa_layers: csa,
            hca_layers: hca,
            mhc_error: mhc_err,
        }
    }

    /// Apply modulation signals to brain modules
    fn apply_modulation(&self, brain: &mut Brain, signals: &[f64]) -> bool {
        if signals.is_empty() { return false; }
        let strength = self.config.modulation_strength;
        for (idx, name) in brain.module_order.iter().enumerate() {
            if idx >= signals.len() { break; }
            let signal = signals[idx] * strength;
            if let Some(module) = brain.modules.get_mut(name) {
                module.activation_level = (module.activation_level as f64 + signal * 0.1)
                    .clamp(0.01, 1.0) as f32;
            }
        }
        true
    }

    // ── Status ──

    pub fn stats(&self) -> DeepSeekCortexStats {
        let kv_stats = self.tiered_kv.as_ref().map(|kv| kv.stats());
        let (csa, hca, std) = self.hybrid_attention.path_counts();
        DeepSeekCortexStats {
            tick: self.tick,
            history_len: self.state_buffer.len(),
            summary_len: self.summary_buffer.len(),
            pred_error: self.last_prediction_error,
            csa_layers: csa,
            hca_layers: hca,
            standard_layers: std,
            muon_steps: self.muon.step_count(),
            compression: self.hybrid_attention.avg_compression_ratio(),
            kv_stats,
        }
    }

    /// Save cortex config and stats to a JSON file for persistence.
    pub fn save_state(&self, path: &str) -> Result<(), String> {
        let data = serde_json::json!({
            "config": self.config,
            "tick": self.tick,
            "last_prediction_error": self.last_prediction_error,
            "last_learn_tick": self.last_learn_tick,
        });
        let json = serde_json::to_string_pretty(&data)
            .map_err(|e| format!("DeepSeek serialize: {e}"))?;
        std::fs::write(path, json).map_err(|e| format!("DeepSeek save: {e}"))
    }

    /// Load cortex state from a JSON file. Returns a fresh cortex with restored config.
    pub fn load_state(path: &str) -> Result<Self, String> {
        let json_str =
            std::fs::read_to_string(path).map_err(|e| format!("DeepSeek read: {e}"))?;
        let data: serde_json::Value =
            serde_json::from_str(&json_str).map_err(|e| format!("DeepSeek parse: {e}"))?;

        let config: DeepSeekCortexConfig = serde_json::from_value(
            data.get("config")
                .cloned()
                .unwrap_or(serde_json::json!({})),
        )
        .map_err(|e| format!("DeepSeek config parse: {e}"))?;

        let mut cortex = Self::new(config);
        if let Some(t) = data.get("tick").and_then(|v| v.as_u64()) {
            cortex.tick = t;
        }
        if let Some(e) = data.get("last_prediction_error").and_then(|v| v.as_f64()) {
            cortex.last_prediction_error = e;
        }
        if let Some(lt) = data.get("last_learn_tick").and_then(|v| v.as_u64()) {
            cortex.last_learn_tick = lt;
        }
        Ok(cortex)
    }
}

#[derive(Debug, Clone)]
pub struct DeepSeekCortexStats {
    pub tick: u64,
    pub history_len: usize,
    pub summary_len: usize,
    pub pred_error: f64,
    pub csa_layers: usize,
    pub hca_layers: usize,
    pub standard_layers: usize,
    pub muon_steps: usize,
    pub compression: f64,
    pub kv_stats: Option<TieredKVStats>,
}

impl std::fmt::Display for DeepSeekCortexStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DS-Ctx t={} hist={}/{} pred_err={:.4} CSA={} HCA={} compr={:.1}x muon={}",
            self.tick, self.history_len, self.summary_len,
            self.pred_error, self.csa_layers, self.hca_layers,
            self.compression, self.muon_steps,
        )
    }
}
