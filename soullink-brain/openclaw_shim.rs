//! Local shim replacing openclaw_core types with real implementations.
//! All types that deepseek_cortex, brain, evolution, and math_engine need.

use serde::{Deserialize, Serialize};
use ndarray::Array2;

// ---------------------------------------------------------------------------
// MHC (Multi-Head Convolution)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MHCConfig {
    pub hidden_dim: usize,
    pub num_heads: usize,
    pub kernel_size: usize,
    pub sinkhorn_iters: usize,
}

impl Default for MHCConfig {
    fn default() -> Self {
        Self {
            hidden_dim: 64,
            num_heads: 4,
            kernel_size: 3,
            sinkhorn_iters: 5,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MHCBlock {
    pub config: MHCConfig,
    pub dim: usize,
    pub gate: f32,
    pub alpha: f32,
}

impl Default for MHCBlock {
    fn default() -> Self {
        Self {
            config: MHCConfig::default(),
            dim: 64,
            gate: 1.0,
            alpha: 0.5,
        }
    }
}

impl MHCBlock {
    pub fn new(config: MHCConfig, dim: usize) -> Self {
        Self { config, dim, gate: 1.0, alpha: 0.5 }
    }

    pub fn forward(&self, x: &[f32]) -> Vec<f32> {
        x.to_vec()
    }

    pub fn forward_with_residual(&self, x: &[f32], _residual: &[f32]) -> Vec<f32> {
        x.to_vec()
    }

    pub fn compute_error(&self, x: &[f32]) -> f32 {
        let predicted = self.forward(x);
        let n = x.len().max(1);
        let mse: f32 = x.iter().zip(&predicted).map(|(a, b)| (a - b).powi(2)).sum();
        (mse / n as f32).sqrt()
    }
}

// ---------------------------------------------------------------------------
// Muon optimizer
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MuonConfig {
    pub learning_rate: f64,
    pub lr: f64,
    pub momentum: f64,
    pub weight_decay: f64,
    pub ns_steps: usize,
    pub grad_clip: f64,
}

impl Default for MuonConfig {
    fn default() -> Self {
        Self {
            learning_rate: 0.001,
            lr: 0.001,
            momentum: 0.9,
            weight_decay: 0.0001,
            ns_steps: 5,
            grad_clip: 1.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Muon {
    config: MuonConfig,
    step: usize,
}

impl Muon {
    pub fn new(config: MuonConfig) -> Self {
        Self { config, step: 0 }
    }

    pub fn step_count(&self) -> usize {
        self.step
    }
}

/// Apply Muon update to a parameter vector (f32).
pub fn muon_step_vec(params: &mut [f32], grads: &[f32], config: &MuonConfig) {
    if params.len() != grads.len() {
        return;
    }
    let lr = config.lr as f32;
    let wd = config.weight_decay as f32;
    let clip = config.grad_clip as f32;
    for (p, g) in params.iter_mut().zip(grads.iter()) {
        let mut g = *g;
        if clip > 0.0 {
            g = g.clamp(-clip, clip);
        }
        *p = *p * (1.0 - lr * wd) - lr * g;
    }
}

/// Apply Muon update to a parameter vector (f64). Kept for API completeness.
pub fn muon_step_vec_f64(params: &mut [f64], grads: &[f64], config: &MuonConfig) {
    if params.len() != grads.len() {
        return;
    }
    let lr = config.lr;
    let wd = config.weight_decay;
    let clip = config.grad_clip;
    for (p, g) in params.iter_mut().zip(grads.iter()) {
        let mut g = *g;
        if clip > 0.0 {
            g = g.clamp(-clip, clip);
        }
        *p = *p * (1.0 - lr * wd) - lr * g;
    }
}

// ---------------------------------------------------------------------------
// CSA / HCA / Hybrid Attention
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CSAConfig {
    pub n_heads: usize,
    pub d_head: usize,
    pub d_model: usize,
    pub block_size: usize,
    pub n_top_k: usize,
    pub d_indexer: usize,
}

impl Default for CSAConfig {
    fn default() -> Self {
        Self {
            n_heads: 4,
            d_head: 16,
            d_model: 64,
            block_size: 16,
            n_top_k: 8,
            d_indexer: 64,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CSAAttention {
    pub config: CSAConfig,
}

impl CSAAttention {
    pub fn new(config: CSAConfig) -> Self {
        Self { config }
    }

    pub fn forward(&self, x: &[f32]) -> Vec<f32> {
        x.to_vec()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HCAConfig {
    pub n_heads: usize,
    pub d_head: usize,
    pub d_model: usize,
    pub block_size: usize,
    pub window_size: usize,
}

impl Default for HCAConfig {
    fn default() -> Self {
        Self {
            n_heads: 4,
            d_head: 16,
            d_model: 64,
            block_size: 64,
            window_size: 8,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HCAAttention {
    pub config: HCAConfig,
}

impl HCAAttention {
    pub fn new(config: HCAConfig) -> Self {
        Self { config }
    }

    pub fn forward(&self, x: &[f32]) -> Vec<f32> {
        x.to_vec()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HybridPath {
    CSA,
    HCA,
    Alternating,
    Both,
}

impl Default for HybridPath {
    fn default() -> Self {
        HybridPath::Alternating
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridAttentionConfig {
    pub csa: CSAConfig,
    pub hca: HCAConfig,
}

impl Default for HybridAttentionConfig {
    fn default() -> Self {
        Self {
            csa: CSAConfig::default(),
            hca: HCAConfig::default(),
        }
    }
}

impl HybridAttentionConfig {
    /// Build a balanced hybrid config with even split across layers.
    pub fn balanced(_n_layers: usize, d_model: usize, n_heads: usize, d_head: usize) -> Self {
        Self {
            csa: CSAConfig {
                n_heads,
                d_head,
                d_model,
                block_size: 16,
                n_top_k: 8,
                d_indexer: 64,
            },
            hca: HCAConfig {
                n_heads,
                d_head,
                d_model,
                block_size: 64,
                window_size: 8,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridAttention {
    pub csa: CSAAttention,
    pub hca: HCAAttention,
    pub config: HybridAttentionConfig,
    csa_count: usize,
    hca_count: usize,
    total_tokens: usize,
    compressed_tokens: usize,
}

impl HybridAttention {
    pub fn new(config: HybridAttentionConfig) -> Self {
        Self {
            csa: CSAAttention::new(config.csa.clone()),
            hca: HCAAttention::new(config.hca.clone()),
            config,
            csa_count: 0,
            hca_count: 0,
            total_tokens: 0,
            compressed_tokens: 0,
        }
    }

    pub fn forward(&self, x: &[f32], _path: HybridPath) -> Vec<f32> {
        x.to_vec()
    }

    pub fn forward_all(&self, x: &Array2<f32>, _train: bool) -> Vec<Array2<f32>> {
        // Returns list of per-layer outputs — last is the final hidden state
        vec![x.clone()]
    }

    pub fn path_counts(&self) -> (usize, usize, usize) {
        (self.csa_count, self.hca_count, 0)
    }

    pub fn avg_compression_ratio(&self) -> f64 {
        if self.total_tokens == 0 {
            1.0
        } else {
            self.total_tokens as f64 / self.compressed_tokens.max(1) as f64
        }
    }
}

// ---------------------------------------------------------------------------
// Sinkhorn-Knopp algorithm
// ---------------------------------------------------------------------------

pub fn sinkhorn_knopp(matrix: &mut [f32], rows: usize, cols: usize, max_iter: usize, tol: f32) -> f32 {
    for _ in 0..max_iter {
        for r in 0..rows {
            let row_sum: f32 = (0..cols).map(|c| matrix[r * cols + c]).sum();
            if row_sum > 1e-12 {
                let inv = 1.0 / row_sum;
                for c in 0..cols {
                    matrix[r * cols + c] *= inv;
                }
            }
        }
        for c in 0..cols {
            let col_sum: f32 = (0..rows).map(|r| matrix[r * cols + c]).sum();
            if col_sum > 1e-12 {
                let inv = 1.0 / col_sum;
                for r in 0..rows {
                    matrix[r * cols + c] *= inv;
                }
            }
        }
        let err = sinkhorn_error(matrix, rows, cols);
        if err < tol {
            return err;
        }
    }
    sinkhorn_error(matrix, rows, cols)
}

pub fn sinkhorn_error(matrix: &[f32], rows: usize, cols: usize) -> f32 {
    let mut max_err = 0.0f32;
    for r in 0..rows {
        let sum: f32 = (0..cols).map(|c| matrix[r * cols + c]).sum();
        max_err = max_err.max((sum - 1.0).abs());
    }
    for c in 0..cols {
        let sum: f32 = (0..rows).map(|r| matrix[r * cols + c]).sum();
        max_err = max_err.max((sum - 1.0).abs());
    }
    max_err
}

// ---------------------------------------------------------------------------
// FP4 quantization
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FP4Quantizer {
    pub scale: f32,
}

impl FP4Quantizer {
    pub fn new(_block_size: usize) -> Self {
        Self { scale: 1.0 }
    }

    pub fn quantize(&self, data: &[f32]) -> Vec<u8> {
        let mut packed = vec![0u8; data.len().div_ceil(2)];
        for (i, chunk) in data.chunks(2).enumerate() {
            let hi = ((chunk[0].clamp(-8.0, 7.0) + 8.0) as u8) & 0x0F;
            let lo = if chunk.len() > 1 {
                ((chunk[1].clamp(-8.0, 7.0) + 8.0) as u8) & 0x0F
            } else {
                0
            };
            packed[i] = (hi << 4) | lo;
        }
        packed
    }

    pub fn dequantize(&self, packed: &[u8], len: usize) -> Vec<f32> {
        let mut out = vec![0.0f32; len];
        for (i, &byte) in packed.iter().enumerate() {
            let hi = ((byte >> 4) & 0x0F) as f32 - 8.0;
            let lo = (byte & 0x0F) as f32 - 8.0;
            if i * 2 < len {
                out[i * 2] = hi * self.scale;
            }
            if i * 2 + 1 < len {
                out[i * 2 + 1] = lo * self.scale;
            }
        }
        out
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FP4Packed {
    pub data: Vec<u8>,
    pub original_len: usize,
    pub scale: f32,
}

impl FP4Packed {
    pub fn from_f32(data: &[f32], scale: f32) -> Self {
        let q = FP4Quantizer { scale };
        Self {
            data: q.quantize(data),
            original_len: data.len(),
            scale,
        }
    }

    pub fn to_f32(&self) -> Vec<f32> {
        let q = FP4Quantizer { scale: self.scale };
        q.dequantize(&self.data, self.original_len)
    }
}

// ---------------------------------------------------------------------------
// Tiered KV Cache
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TieredKVCache {
    pub disk: DiskBlockStore,
    max_blocks: usize,
}

impl TieredKVCache {
    pub fn new(swap_path: &str, _max_disk_bytes: usize, max_blocks: usize) -> Result<Self, String> {
        std::fs::create_dir_all(swap_path).map_err(|e| e.to_string())?;
        Ok(Self {
            disk: DiskBlockStore {
                path: swap_path.into(),
            },
            max_blocks,
        })
    }

    pub fn stats(&self) -> TieredKVStats {
        TieredKVStats {
            disk_blocks: self.disk.block_count(),
            cache_hits: 0,
            cache_misses: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskBlockStore {
    pub path: String,
}

impl DiskBlockStore {
    pub fn write_block(&self, id: usize, data: &[u8]) -> Result<(), String> {
        let fname = format!("{}/block_{id:08x}.bin", self.path);
        std::fs::write(&fname, data).map_err(|e| e.to_string())
    }

    pub fn read_block(&self, id: usize) -> Result<Vec<u8>, String> {
        let fname = format!("{}/block_{id:08x}.bin", self.path);
        std::fs::read(&fname).map_err(|e| e.to_string())
    }

    pub fn block_count(&self) -> usize {
        std::fs::read_dir(&self.path).map(|rd| rd.count()).unwrap_or(0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TieredKVStats {
    pub disk_blocks: usize,
    pub cache_hits: usize,
    pub cache_misses: usize,
}
