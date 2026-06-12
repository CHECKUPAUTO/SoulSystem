//! Types for soullink-inference — request/response, priority, quantization, targets.

use serde::{Deserialize, Serialize};

/// Inference priority levels (lower = higher priority).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Priority {
    /// User-facing request — minimal latency, GPU preferred.
    Think,
    /// Background embedding batch — throughput-optimized.
    Embed,
    /// Autonomous dream cycle — latency-tolerant, CPU preferred.
    Dream,
    /// GPU preprocessing pipeline (DALI) — image/PDF decode, resize, normalize.
    Preprocess,
    /// Vector similarity search (CUTLASS) — cosine sim + top-K on GPU.
    VectorSearch,
    /// HNN integration step (CUTLASS) — Verlet step on GPU.
    HnnStep,
}

/// Quantization levels for model weights.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Quantization {
    /// Full float16 — fine-tuning only, ~2 bytes/param.
    F16,
    /// 8-bit quantization — high quality embeddings.
    Q8_0,
    /// 4-bit medium — balanced quality/speed for Think.
    Q4_K_M,
    /// 2-bit K — smallest, Dream mode on CPU.
    Q2_K,
}

impl Quantization {
    /// Estimated bytes per parameter for this quantization.
    pub fn bytes_per_param(&self) -> f64 {
        match self {
            Quantization::F16 => 2.0,
            Quantization::Q8_0 => 1.0,
            Quantization::Q4_K_M => 0.5625, // 4.5 bits
            Quantization::Q2_K => 0.3125,   // 2.5 bits
        }
    }

    /// Estimated model size in GB for a given parameter count.
    pub fn model_size_gb(&self, params_b: f64) -> f64 {
        params_b * 1e9 * self.bytes_per_param() / 1e9
    }
}

/// Where to execute inference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionTarget {
    /// Execute on GPU (RTX 4060) with specified quantization.
    Gpu(Quantization),
    /// Execute on CPU pinned to a specific NUMA node with quantization.
    CpuNuma { node: u32, quant: Quantization },
    /// Fallback to Ollama HTTP API.
    FallbackOllama,
    /// DALI GPU preprocessing pipeline (image decode, resize, normalize).
    DaliGpu,
    /// CUTLASS vector search (cosine similarity + top-K).
    CutlassSearch,
    /// CUTLASS HNN Verlet integration step.
    CutlassHnn,
    /// VRAM insufficient — wait for Ollama to unload, then retry.
    VramWait,
}

/// A generation request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateRequest {
    pub model: String,
    pub prompt: String,
    pub max_tokens: usize,
    pub temperature: f32,
    pub priority: Priority,
    pub quantization: Option<Quantization>,
}

/// A generation response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateResponse {
    pub text: String,
    pub model: String,
    pub target: String,       // "gpu", "cpu-numa-0", "ollama"
    pub quantization: String, // "Q4_K_M", "Q2_K"
    pub tokens_generated: usize,
    pub latency_ms: u64,
}

/// An embedding request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbedRequest {
    pub model: String,
    pub texts: Vec<String>,
}

/// An embedding response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbedResponse {
    pub embeddings: Vec<Vec<f32>>,
    pub model: String,
    pub dimensions: usize,
    pub latency_ms: u64,
}

/// Hardware snapshot for routing decisions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareSnapshot {
    pub numa_nodes: Vec<NumaNodeStatus>,
    pub gpu: GpuStatus,
    pub timestamp_ms: u64,
}

/// Status of a single NUMA node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NumaNodeStatus {
    pub node_id: u32,
    pub cpu_cores: Vec<u32>,
    pub memory_total_gb: f64,
    pub memory_free_gb: f64,
    pub l3_cache_kb: usize,
}

/// GPU status from NVML.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuStatus {
    pub vram_total_gb: f64,
    pub vram_free_gb: f64,
    pub gpu_temp_c: i32,
    pub sm_clock_mhz: u32,
    pub max_sm_clock_mhz: u32,
    pub throttle_active: bool,
}

/// Inference engine configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceConfig {
    /// Ollama base URL for fallback.
    pub ollama_url: String,
    /// Ollama timeout in seconds.
    pub ollama_timeout_secs: u64,
    /// GPU VRAM headroom to reserve (GB).
    pub vram_headroom_gb: f64,
    /// CPU RAM headroom to reserve per NUMA node (GB).
    pub ram_headroom_gb: f64,
    /// Maximum concurrent inference tasks.
    pub max_concurrent: usize,
    /// Warm standby enabled.
    pub warm_standby: bool,
}

impl Default for InferenceConfig {
    fn default() -> Self {
        Self {
            ollama_url: "http://127.0.0.1:11434".to_string(),
            ollama_timeout_secs: 120,
            vram_headroom_gb: 1.0,
            ram_headroom_gb: 8.0,
            max_concurrent: 4,
            warm_standby: true,
        }
    }
}
