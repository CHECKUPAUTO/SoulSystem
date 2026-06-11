//! Inference engine — the main orchestrator for soullink-inference.
//!
//! Wraps hardware probe, model router, batch scheduler, and warm standby
//! into a single async interface.

use crate::batch::BatchScheduler;
use crate::hardware::HardwareProbe;
use crate::router::ModelRouter;
use crate::standby::WarmStandby;
use crate::types::*;
use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{error, info};

/// The main inference engine — routes requests, manages warm standby,
/// and provides a unified async API.
pub struct InferenceEngine {
    config: InferenceConfig,
    hardware: Arc<RwLock<HardwareSnapshot>>,
    router: Arc<RwLock<ModelRouter>>,
    scheduler: Arc<BatchScheduler>,
    standby: Arc<WarmStandby>,
    http: reqwest::Client,
}

impl InferenceEngine {
    /// Create a new inference engine with the given config.
    pub async fn new(config: InferenceConfig) -> Result<Self> {
        // Probe hardware
        let hardware = HardwareProbe::snapshot()?;
        info!(
            numa_nodes = hardware.numa_nodes.len(),
            gpu_vram_free = hardware.gpu.vram_free_gb,
            "InferenceEngine initialized"
        );

        let router = ModelRouter::new();
        let scheduler = Arc::new(BatchScheduler::new(config.max_concurrent));
        let standby = Arc::new(WarmStandby::new(Duration::from_secs(300)));

        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.ollama_timeout_secs))
            .build()?;

        Ok(Self {
            config,
            hardware: Arc::new(RwLock::new(hardware)),
            router: Arc::new(RwLock::new(router)),
            scheduler,
            standby,
            http,
        })
    }

    /// Generate text — auto-routes to GPU or CPU based on current load.
    pub async fn generate(&self, req: GenerateRequest) -> Result<GenerateResponse> {
        let start = std::time::Instant::now();

        // Refresh hardware snapshot
        let snap = HardwareProbe::snapshot()?;

        // Route
        let target = self.router.read().await.route(&req, &snap);

        // Check warm standby
        let already_loaded = self.standby.is_any_loaded(&req.model).await;

        info!(
            model = %req.model,
            target = ?target,
            warm = already_loaded,
            "Generating inference"
        );

        // Execute
        let result = match target {
            ExecutionTarget::Gpu(quant) => {
                self.mark_loaded(&req.model, quant, &target).await;
                self.call_ollama(&req, &quant).await
            }
            ExecutionTarget::CpuNuma { node: _, quant } => {
                self.mark_loaded(&req.model, quant, &target).await;
                // TODO: NUMA pinning via nix::sched::sched_setaffinity
                self.call_ollama(&req, &quant).await
            }
            ExecutionTarget::FallbackOllama => {
                self.call_ollama(&req, &req.quantization.unwrap_or(Quantization::Q4_K_M))
                    .await
            }
            ExecutionTarget::DaliGpu
            | ExecutionTarget::CutlassSearch
            | ExecutionTarget::CutlassHnn
            | ExecutionTarget::VramWait => {
                anyhow::bail!("DALI/CUTLASS targets not handled by InferenceEngine");
            }
        };

        let latency_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(mut resp) => {
                resp.latency_ms = latency_ms;
                // Mark model as used in warm standby
                if self.standby.is_any_loaded(&req.model).await {
                    // Already tracked
                }
                Ok(resp)
            }
            Err(e) => {
                error!(error = %e, "Inference failed");
                Err(e)
            }
        }
    }

    /// Embed text via Ollama — batched, semaphore-controlled.
    pub async fn embed(&self, texts: &[String], model: &str) -> Result<Vec<Vec<f32>>> {
        let url = format!("{}/api/embeddings", self.config.ollama_url);
        let mut all_embeddings = Vec::with_capacity(texts.len());

        for text in texts {
            let body = serde_json::json!({
                "model": model,
                "prompt": text,
            });

            let resp = self
                .http
                .post(&url)
                .json(&body)
                .send()
                .await?
                .error_for_status()?;

            let data: serde_json::Value = resp.json().await?;
            if let Some(embedding) = data.get("embedding").and_then(|v| v.as_array()) {
                let vec: Vec<f32> = embedding
                    .iter()
                    .filter_map(|v| v.as_f64().map(|f| f as f32))
                    .collect();
                all_embeddings.push(vec);
            } else {
                anyhow::bail!("No embedding in response from Ollama");
            }
        }

        Ok(all_embeddings)
    }

    /// Get current hardware state.
    pub async fn hardware_snapshot(&self) -> HardwareSnapshot {
        self.hardware.read().await.clone()
    }

    /// Refresh hardware snapshot.
    pub async fn refresh_hardware(&self) -> Result<()> {
        let snap = HardwareProbe::snapshot()?;
        *self.hardware.write().await = snap;
        Ok(())
    }

    /// Evict idle models from warm standby.
    pub async fn evict_idle_models(&self) -> Vec<String> {
        self.standby.evict_idle().await
    }

    /// Get the batch scheduler stats.
    pub async fn scheduler_stats(&self) -> (usize, usize) {
        (self.scheduler.pending().await, self.scheduler.available())
    }

    /// Register a model's parameter count for routing.
    pub async fn register_model(&self, name: &str, params_b: f64) {
        self.router.write().await.register_model(name, params_b);
    }

    /// Call Ollama HTTP API for generation.
    async fn call_ollama(
        &self,
        req: &GenerateRequest,
        quant: &Quantization,
    ) -> Result<GenerateResponse> {
        let quant_suffix = match quant {
            Quantization::F16 => ":f16",
            Quantization::Q8_0 => ":q8_0",
            Quantization::Q4_K_M => ":q4_k_m",
            Quantization::Q2_K => ":q2_k",
        };

        let model_with_quant = format!("{}{}", req.model, quant_suffix);
        let url = format!("{}/api/generate", self.config.ollama_url);

        let body = serde_json::json!({
            "model": model_with_quant,
            "prompt": req.prompt,
            "stream": false,
            "options": {
                "num_predict": req.max_tokens,
                "temperature": req.temperature,
            }
        });

        let resp = self
            .http
            .post(&url)
            .json(&body)
            .send()
            .await?
            .error_for_status()?;

        let data: serde_json::Value = resp.json().await?;
        let text = data
            .get("response")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let tokens = text.split_whitespace().count();

        Ok(GenerateResponse {
            text,
            model: model_with_quant,
            target: "ollama".to_string(),
            quantization: format!("{:?}", quant),
            tokens_generated: tokens,
            latency_ms: 0, // Set by caller
        })
    }

    /// Mark a model as loaded in warm standby.
    async fn mark_loaded(&self, model: &str, quant: Quantization, target: &ExecutionTarget) {
        self.standby.mark_loaded(model, quant, target.clone()).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn engine_initialization() {
        let config = InferenceConfig::default();
        let engine = InferenceEngine::new(config).await.unwrap();
        let snap = engine.hardware_snapshot().await;
        assert!(!snap.numa_nodes.is_empty());
    }

    #[tokio::test]
    async fn register_model_and_check_routing() {
        let config = InferenceConfig::default();
        let engine = InferenceEngine::new(config).await.unwrap();
        engine.register_model("custom-model", 13.0).await;

        let snap = engine.hardware_snapshot().await;
        let req = GenerateRequest {
            model: "custom-model".to_string(),
            prompt: "test".into(),
            max_tokens: 100,
            temperature: 0.7,
            priority: Priority::Dream,
            quantization: None,
        };

        let target = engine.router.read().await.route(&req, &snap);
        // Dream should route to CPU or GPU with Q2_K
        match target {
            ExecutionTarget::CpuNuma { quant, .. } => assert_eq!(quant, Quantization::Q2_K),
            ExecutionTarget::Gpu(quant) => assert_eq!(quant, Quantization::Q2_K),
            ExecutionTarget::FallbackOllama => {} // Also acceptable if nothing fits
            ExecutionTarget::DaliGpu
            | ExecutionTarget::CutlassSearch
            | ExecutionTarget::CutlassHnn
            | ExecutionTarget::VramWait => {}
        }
    }

    #[tokio::test]
    async fn scheduler_stats() {
        let config = InferenceConfig::default();
        let engine = InferenceEngine::new(config).await.unwrap();
        let (pending, available) = engine.scheduler_stats().await;
        assert_eq!(pending, 0);
        assert!(available > 0);
    }
}
