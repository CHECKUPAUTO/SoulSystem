//! Model router — decides GPU vs CPU NUMA vs Ollama vs DALI/CUTLASS.
//!
//! Routing strategy:
//! - Think (user-facing) → GPU if VRAM fits, else CPU NUMA-0
//! - Dream (background) → CPU NUMA-1 with Q2_K
//! - Embed (batch) → GPU with Q8_0 if available, else CPU
//! - Preprocess (DALI) → GPU if VRAM > 2GB free, else CPU fallback
//! - VectorSearch (CUTLASS) → GPU if VRAM > 1GB free, else CPU AVX2
//! - HnnStep (CUTLASS) → GPU always (tiny VRAM footprint)

use crate::types::*;
use std::collections::HashMap;
use tracing::{info, warn};

/// Known model sizes in billions of parameters.
const MODEL_SIZES: &[(&str, f64)] = &[
    ("nomic-embed-text", 0.137), // 137M
    ("qwen2.5-coder:3b", 3.0),
    ("gemma4:31b", 31.0),
    ("llama3:8b", 8.0),
    ("mistral:7b", 7.0),
];

/// Minimum free VRAM thresholds for GPU operations.
const VRAM_MIN_PREPROCESS_GB: f64 = 2.0; // DALI needs ~2GB
const VRAM_MIN_SEARCH_GB: f64 = 1.0; // CUTLASS cosine sim
const VRAM_MIN_HNN_GB: f64 = 0.1; // HNN step is tiny
const VRAM_HEADROOM_GB: f64 = 1.0; // Safety margin for LLM

pub struct ModelRouter {
    model_sizes: HashMap<String, f64>,
}

impl Default for ModelRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelRouter {
    pub fn new() -> Self {
        let mut model_sizes = HashMap::new();
        for (name, size) in MODEL_SIZES {
            model_sizes.insert(name.to_string(), *size);
        }
        Self { model_sizes }
    }

    pub fn register_model(&mut self, name: &str, params_b: f64) {
        self.model_sizes.insert(name.to_string(), params_b);
    }

    /// Route a generation request to the best execution target.
    pub fn route(&self, req: &GenerateRequest, snap: &HardwareSnapshot) -> ExecutionTarget {
        let vram_free = snap.gpu.vram_free_gb;

        match req.priority {
            Priority::Preprocess => return self.route_preprocess(vram_free),
            Priority::VectorSearch => return self.route_vector_search(vram_free),
            Priority::HnnStep => return self.route_hnn_step(vram_free),
            _ => {} // Fall through to LLM routing
        }

        // LLM routing
        let model_size = self.model_sizes.get(&req.model).copied().unwrap_or(7.0);
        let quant = req
            .quantization
            .unwrap_or_else(|| Self::default_quant(req.priority));
        let model_size_gb = quant.model_size_gb(model_size);

        info!(
            model = %req.model,
            priority = ?req.priority,
            quant = ?quant,
            size_gb = model_size_gb,
            vram_free_gb = vram_free,
            "Routing inference request"
        );

        // Think → GPU preferred
        if req.priority == Priority::Think {
            if vram_free > model_size_gb + VRAM_HEADROOM_GB && !snap.gpu.throttle_active {
                return ExecutionTarget::Gpu(quant);
            }
            if let Some(node) = Self::best_numa_node(snap, model_size_gb) {
                warn!(
                    "GPU unavailable for Think — falling back to CPU NUMA-{}",
                    node
                );
                return ExecutionTarget::CpuNuma { node, quant };
            }
            return ExecutionTarget::FallbackOllama;
        }

        // Dream → CPU preferred
        if req.priority == Priority::Dream {
            let dream_quant = Quantization::Q2_K;
            let dream_size_gb = dream_quant.model_size_gb(model_size);
            if let Some(node) = Self::best_numa_node(snap, dream_size_gb) {
                return ExecutionTarget::CpuNuma {
                    node,
                    quant: dream_quant,
                };
            }
            if vram_free > dream_size_gb + VRAM_HEADROOM_GB {
                return ExecutionTarget::Gpu(dream_quant);
            }
            return ExecutionTarget::FallbackOllama;
        }

        // Embed → GPU preferred for throughput
        if req.priority == Priority::Embed {
            if vram_free > model_size_gb + VRAM_HEADROOM_GB {
                return ExecutionTarget::Gpu(Quantization::Q8_0);
            }
            if let Some(node) = Self::best_numa_node(snap, model_size_gb) {
                return ExecutionTarget::CpuNuma {
                    node,
                    quant: Quantization::Q8_0,
                };
            }
            return ExecutionTarget::FallbackOllama;
        }

        ExecutionTarget::FallbackOllama
    }

    /// DALI preprocessing: GPU if enough VRAM, else signal wait.
    fn route_preprocess(&self, vram_free: f64) -> ExecutionTarget {
        if vram_free >= VRAM_MIN_PREPROCESS_GB {
            info!(vram_free_gb = vram_free, "DALI Preprocess → GPU");
            ExecutionTarget::DaliGpu
        } else {
            warn!(
                vram_free_gb = vram_free,
                "VRAM too low for DALI — waiting for Ollama purge"
            );
            ExecutionTarget::VramWait
        }
    }

    /// CUTLASS vector search: GPU if enough VRAM, else signal wait.
    fn route_vector_search(&self, vram_free: f64) -> ExecutionTarget {
        if vram_free >= VRAM_MIN_SEARCH_GB {
            info!(vram_free_gb = vram_free, "CUTLASS VectorSearch → GPU");
            ExecutionTarget::CutlassSearch
        } else {
            warn!(
                vram_free_gb = vram_free,
                "VRAM too low for CUTLASS search — waiting"
            );
            ExecutionTarget::VramWait
        }
    }

    /// CUTLASS HNN step: GPU almost always (tiny footprint).
    fn route_hnn_step(&self, vram_free: f64) -> ExecutionTarget {
        if vram_free >= VRAM_MIN_HNN_GB {
            ExecutionTarget::CutlassHnn
        } else {
            warn!("Even HNN can't fit in VRAM — GPU critically full");
            ExecutionTarget::VramWait
        }
    }

    fn default_quant(priority: Priority) -> Quantization {
        match priority {
            Priority::Think => Quantization::Q4_K_M,
            Priority::Embed => Quantization::Q8_0,
            Priority::Dream => Quantization::Q2_K,
            Priority::Preprocess | Priority::VectorSearch | Priority::HnnStep => {
                Quantization::Q4_K_M
            }
        }
    }

    fn best_numa_node(snap: &HardwareSnapshot, model_size_gb: f64) -> Option<u32> {
        let headroom = 8.0;
        snap.numa_nodes
            .iter()
            .filter(|n| n.memory_free_gb > model_size_gb + headroom)
            .max_by(|a, b| {
                a.memory_free_gb
                    .partial_cmp(&b.memory_free_gb)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|n| n.node_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_snap(vram_free: f64, numa_free: &[f64]) -> HardwareSnapshot {
        HardwareSnapshot {
            numa_nodes: numa_free
                .iter()
                .enumerate()
                .map(|(i, free)| NumaNodeStatus {
                    node_id: i as u32,
                    cpu_cores: vec![],
                    memory_total_gb: 64.0,
                    memory_free_gb: *free,
                    l3_cache_kb: 30720,
                })
                .collect(),
            gpu: GpuStatus {
                vram_total_gb: 8.0,
                vram_free_gb: vram_free,
                gpu_temp_c: 70,
                sm_clock_mhz: 2100,
                max_sm_clock_mhz: 2790,
                throttle_active: false,
            },
            timestamp_ms: 0,
        }
    }

    #[test]
    fn think_routes_to_gpu_when_vram_available() {
        let router = ModelRouter::new();
        let snap = make_snap(6.0, &[20.0, 5.0]);
        let req = GenerateRequest {
            model: "qwen2.5-coder:3b".to_string(),
            prompt: "test".into(),
            max_tokens: 100,
            temperature: 0.7,
            priority: Priority::Think,
            quantization: None,
        };
        assert!(matches!(router.route(&req, &snap), ExecutionTarget::Gpu(_)));
    }

    #[test]
    fn think_falls_back_to_cpu_when_vram_full() {
        let router = ModelRouter::new();
        let snap = make_snap(0.5, &[30.0, 5.0]);
        let req = GenerateRequest {
            model: "qwen2.5-coder:3b".to_string(),
            prompt: "test".into(),
            max_tokens: 100,
            temperature: 0.7,
            priority: Priority::Think,
            quantization: None,
        };
        match router.route(&req, &snap) {
            ExecutionTarget::CpuNuma { node, .. } => assert_eq!(node, 0),
            other => panic!("Expected CpuNuma, got {:?}", other),
        }
    }

    #[test]
    fn dream_routes_to_cpu_with_q2k() {
        let router = ModelRouter::new();
        let snap = make_snap(6.0, &[20.0, 5.0]);
        let req = GenerateRequest {
            model: "qwen2.5-coder:3b".to_string(),
            prompt: "dream".into(),
            max_tokens: 50,
            temperature: 1.0,
            priority: Priority::Dream,
            quantization: None,
        };
        match router.route(&req, &snap) {
            ExecutionTarget::CpuNuma { quant, .. } => assert_eq!(quant, Quantization::Q2_K),
            ExecutionTarget::Gpu(quant) => assert_eq!(quant, Quantization::Q2_K),
            other => panic!("Expected CpuNuma or Gpu with Q2_K, got {:?}", other),
        }
    }

    #[test]
    fn embed_routes_to_gpu_with_q8() {
        let router = ModelRouter::new();
        let snap = make_snap(6.0, &[20.0, 5.0]);
        let req = GenerateRequest {
            model: "nomic-embed-text".to_string(),
            prompt: "embed".into(),
            max_tokens: 0,
            temperature: 0.0,
            priority: Priority::Embed,
            quantization: None,
        };
        if let ExecutionTarget::Gpu(q) = router.route(&req, &snap) {
            assert_eq!(q, Quantization::Q8_0)
        }
    }

    #[test]
    fn throttle_forces_cpu_fallback() {
        let router = ModelRouter::new();
        let mut snap = make_snap(6.0, &[30.0, 5.0]);
        snap.gpu.throttle_active = true;
        snap.gpu.gpu_temp_c = 87;
        let req = GenerateRequest {
            model: "qwen2.5-coder:3b".to_string(),
            prompt: "test".into(),
            max_tokens: 100,
            temperature: 0.7,
            priority: Priority::Think,
            quantization: None,
        };
        assert!(matches!(
            router.route(&req, &snap),
            ExecutionTarget::CpuNuma { .. }
        ));
    }

    #[test]
    fn ollama_fallback_when_nothing_fits() {
        let router = ModelRouter::new();
        let snap = make_snap(0.1, &[0.5, 0.5]);
        let req = GenerateRequest {
            model: "gemma4:31b".to_string(),
            prompt: "huge".into(),
            max_tokens: 100,
            temperature: 0.7,
            priority: Priority::Think,
            quantization: Some(Quantization::F16),
        };
        assert!(matches!(
            router.route(&req, &snap),
            ExecutionTarget::FallbackOllama
        ));
    }

    // ── New DALI/CUTLASS routing tests ──────────────────────────────────────

    #[test]
    fn preprocess_routes_to_dali_gpu_when_vram_available() {
        let router = ModelRouter::new();
        let snap = make_snap(3.0, &[20.0, 5.0]); // 3GB free > 2GB threshold
        let req = GenerateRequest {
            model: "dali".to_string(),
            prompt: "preprocess".into(),
            max_tokens: 0,
            temperature: 0.0,
            priority: Priority::Preprocess,
            quantization: None,
        };
        assert!(matches!(
            router.route(&req, &snap),
            ExecutionTarget::DaliGpu
        ));
    }

    #[test]
    fn preprocess_returns_vram_wait_when_low() {
        let router = ModelRouter::new();
        let snap = make_snap(1.0, &[20.0, 5.0]); // Only 1GB free < 2GB threshold
        let req = GenerateRequest {
            model: "dali".to_string(),
            prompt: "preprocess".into(),
            max_tokens: 0,
            temperature: 0.0,
            priority: Priority::Preprocess,
            quantization: None,
        };
        assert!(matches!(
            router.route(&req, &snap),
            ExecutionTarget::VramWait
        ));
    }

    #[test]
    fn vector_search_routes_to_cutlass_when_vram_available() {
        let router = ModelRouter::new();
        let snap = make_snap(2.0, &[20.0, 5.0]); // 2GB free > 1GB threshold
        let req = GenerateRequest {
            model: "cutlass".to_string(),
            prompt: "search".into(),
            max_tokens: 0,
            temperature: 0.0,
            priority: Priority::VectorSearch,
            quantization: None,
        };
        assert!(matches!(
            router.route(&req, &snap),
            ExecutionTarget::CutlassSearch
        ));
    }

    #[test]
    fn vector_search_returns_vram_wait_when_low() {
        let router = ModelRouter::new();
        let snap = make_snap(0.5, &[20.0, 5.0]); // 0.5GB < 1GB threshold
        let req = GenerateRequest {
            model: "cutlass".to_string(),
            prompt: "search".into(),
            max_tokens: 0,
            temperature: 0.0,
            priority: Priority::VectorSearch,
            quantization: None,
        };
        assert!(matches!(
            router.route(&req, &snap),
            ExecutionTarget::VramWait
        ));
    }

    #[test]
    fn hnn_step_routes_to_cutlass_even_with_minimal_vram() {
        let router = ModelRouter::new();
        let snap = make_snap(0.2, &[20.0, 5.0]); // 200MB > 100MB threshold
        let req = GenerateRequest {
            model: "hnn".to_string(),
            prompt: "step".into(),
            max_tokens: 0,
            temperature: 0.0,
            priority: Priority::HnnStep,
            quantization: None,
        };
        assert!(matches!(
            router.route(&req, &snap),
            ExecutionTarget::CutlassHnn
        ));
    }
}
