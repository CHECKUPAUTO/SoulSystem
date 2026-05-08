//! TurboQuant Proxy Server — HTTP reverse proxy with KV cache compression.
//!
//! Sits between clients and llama-server, adding 3-bit KV offload:
//! ```text
//! Client → :8082 (proxy) → :8081 (llama-server Q4_0)
//!                ↓
//!         KV Bridge (offload to TurboQuant 3-bit when GPU full)
//! ```

use crate::turboquant::proxy::kv_bridge::KVBridge;
use crate::turboquant::proxy::metrics::ProxyMetrics;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Serialize, Deserialize)]
pub struct ProxyConfig {
    pub listen_port: u16,
    pub llama_server_url: String,
    pub gpu_kv_capacity: usize,
    pub offload_threshold: f64,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            listen_port: 8082,
            llama_server_url: "http://localhost:8081".into(),
            gpu_kv_capacity: 4096,
            offload_threshold: 0.8,
        }
    }
}

pub struct TurboQuantProxy {
    config: ProxyConfig,
    bridge: Arc<KVBridge>,
    metrics: Arc<RwLock<ProxyMetrics>>,
    http_client: Client,
}

impl TurboQuantProxy {
    pub fn new(config: ProxyConfig) -> Self {
        let bridge = Arc::new(KVBridge::new(
            24,   // num_layers (Qwen3-8B)
            8192, // max_seq_len
            64,   // head_dim
            8,    // num_heads
            config.gpu_kv_capacity,
            config.llama_server_url.clone(),
        ));

        Self {
            config,
            bridge,
            metrics: Arc::new(RwLock::new(ProxyMetrics::default())),
            http_client: Client::new(),
        }
    }

    /// Forward a completion request to llama-server.
    pub async fn forward_completion(
        &self,
        body: &str,
    ) -> Result<String, anyhow::Error> {
        let url = format!("{}/v1/chat/completions", self.config.llama_server_url);
        let resp = self.http_client
            .post(&url)
            .header("Content-Type", "application/json")
            .body(body.to_string())
            .send()
            .await?;

        let status = resp.status();
        let text = resp.text().await?;

        // Update metrics
        let mut metrics = self.metrics.write().await;
        metrics.total_requests += 1;
        if status.is_success() {
            metrics.successful_requests += 1;
        } else {
            metrics.failed_requests += 1;
        }

        Ok(text)
    }

    /// Check llama-server health.
    pub async fn health_check(&self) -> bool {
        let url = format!("{}/health", self.config.llama_server_url);
        self.http_client.get(&url).send().await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    /// Get combined stats: llama-server + TurboQuant offload.
    pub async fn stats(&self) -> ProxyStats {
        let bridge_stats = self.bridge.stats().await;
        let metrics = self.metrics.read().await;
        let healthy = self.health_check().await;

        ProxyStats {
            llama_server_healthy: healthy,
            total_requests: metrics.total_requests,
            successful_requests: metrics.successful_requests,
            failed_requests: metrics.failed_requests,
            gpu_kv_positions: bridge_stats.gpu_positions,
            turboquant_positions: bridge_stats.turboquant_positions,
            turboquant_memory_mib: bridge_stats.memory_turboquant_mib,
            compression_ratio: bridge_stats.compression_ratio,
            effective_compression_vs_fp16: bridge_stats.effective_vs_fp16,
        }
    }

    pub fn bridge(&self) -> Arc<KVBridge> { self.bridge.clone() }
    pub fn config(&self) -> &ProxyConfig { &self.config }
    pub fn http_client(&self) -> &Client { &self.http_client }
}

#[derive(Debug, Clone, Serialize)]
pub struct ProxyStats {
    pub llama_server_healthy: bool,
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub gpu_kv_positions: usize,
    pub turboquant_positions: usize,
    pub turboquant_memory_mib: f64,
    pub compression_ratio: f64,
    pub effective_compression_vs_fp16: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config() {
        let config = ProxyConfig::default();
        assert_eq!(config.listen_port, 8082);
        assert_eq!(config.offload_threshold, 0.8);
    }

    #[tokio::test]
    async fn proxy_creation() {
        let proxy = TurboQuantProxy::new(ProxyConfig::default());
        assert_eq!(proxy.config().listen_port, 8082);
    }
}
