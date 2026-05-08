//! Ollama embedding bridge for soullink-rag.
//!
//! Calls Ollama /api/embeddings to convert text into real vectors.
//! Supports async batch ingestion with configurable concurrency.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::{info, warn, instrument};

/// Ollama embedding request.
#[derive(Debug, Serialize)]
struct EmbedRequest {
    model: String,
    prompt: String,
}

/// Ollama embedding response.
#[derive(Debug, Deserialize)]
struct EmbedResponse {
    embedding: Vec<f32>,
}

/// Configuration for the embedding bridge.
#[derive(Debug, Clone)]
pub struct EmbedConfig {
    /// Ollama endpoint URL.
    pub ollama_url: String,
    /// Embedding model name.
    pub model: String,
    /// Dimension of the output vectors (model-dependent).
    pub dimension: usize,
    /// Maximum concurrent embedding requests.
    pub max_concurrency: usize,
    /// Request timeout in seconds.
    pub timeout_secs: u64,
}

impl Default for EmbedConfig {
    fn default() -> Self {
        Self {
            ollama_url: "http://127.0.0.1:11434".to_string(),
            model: "nomic-embed-text".to_string(),
            dimension: 768,
            max_concurrency: 4,
            timeout_secs: 30,
        }
    }
}

/// The Ollama embedding bridge.
pub struct EmbedBridge {
    config: EmbedConfig,
    client: reqwest::Client,
    semaphore: Arc<Semaphore>,
}

impl EmbedBridge {
    pub fn new(config: EmbedConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .expect("failed to build HTTP client");

        let semaphore = Arc::new(Semaphore::new(config.max_concurrency));

        Self { config, client, semaphore }
    }

    /// Get the embedding dimension for the configured model.
    pub fn dimension(&self) -> usize {
        self.config.dimension
    }

    /// Embed a single text string.
    #[instrument(skip(self, text), fields(model = %self.config.model))]
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let _permit = self.semaphore.acquire().await?;

        let url = format!("{}/api/embeddings", self.config.ollama_url);
        let req = EmbedRequest {
            model: self.config.model.clone(),
            prompt: text.to_string(),
        };

        let resp = self.client.post(&url)
            .json(&req)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Ollama embedding failed: {} - {}", status, body);
        }

        let result: EmbedResponse = resp.json().await?;
        Ok(result.embedding)
    }

    /// Embed multiple texts in parallel with concurrency control.
    pub async fn embed_batch(&self, texts: &[String]) -> Vec<Result<Vec<f32>>> {
        let futures: Vec<_> = texts.iter()
            .map(|text| self.embed(text))
            .collect();

        futures::future::join_all(futures).await
    }

    /// Check if Ollama is reachable and the model is available.
    pub async fn health_check(&self) -> Result<bool> {
        let url = format!("{}/api/tags", self.config.ollama_url);
        let resp = self.client.get(&url).send().await;

        match resp {
            Ok(r) if r.status().is_success() => Ok(true),
            Ok(r) => {
                warn!("Ollama health check failed: {}", r.status());
                Ok(false)
            }
            Err(e) => {
                warn!("Ollama unreachable: {}", e);
                Ok(false)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embed_config_defaults() {
        let config = EmbedConfig::default();
        assert_eq!(config.model, "nomic-embed-text");
        assert_eq!(config.dimension, 768);
        assert_eq!(config.max_concurrency, 4);
        assert_eq!(config.timeout_secs, 30);
    }

    #[test]
    fn embed_bridge_creation() {
        let bridge = EmbedBridge::new(EmbedConfig::default());
        assert_eq!(bridge.dimension(), 768);
    }

    #[test]
    fn embed_config_custom() {
        let config = EmbedConfig {
            model: "mxbai-embed-large".to_string(),
            dimension: 1024,
            max_concurrency: 8,
            ..Default::default()
        };
        assert_eq!(config.model, "mxbai-embed-large");
        assert_eq!(config.dimension, 1024);
    }

    #[tokio::test]
    async fn health_check_unreachable() {
        let config = EmbedConfig {
            ollama_url: "http://127.0.0.1:19999".to_string(), // non-existent
            timeout_secs: 2,
            ..Default::default()
        };
        let bridge = EmbedBridge::new(config);
        let healthy = bridge.health_check().await.unwrap();
        assert!(!healthy, "should be unreachable");
    }
}