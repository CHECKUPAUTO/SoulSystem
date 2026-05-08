//! VisionPipeline — orchestrates image embedding via Ollama on RTX 4060.

use crate::embedding::{EmbeddingBackend, ImageEmbedding};
use anyhow::Result;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde::{Deserialize, Serialize};
use std::time::Instant;
use tracing::{info, instrument};

/// Ollama embedding request for images.
#[derive(Debug, Serialize)]
struct OllamaEmbedRequest {
    model: String,
    input: String,
}

/// Ollama embedding response.
#[derive(Debug, Deserialize)]
struct OllamaEmbedResponse {
    embeddings: Vec<Vec<f32>>,
}

/// Vision pipeline configuration.
#[derive(Debug, Clone)]
pub struct VisionConfig {
    /// Ollama endpoint.
    pub ollama_url: String,
    /// Embedding backend to use.
    pub backend: EmbeddingBackend,
    /// Maximum image size in bytes (default: 10MB).
    pub max_image_bytes: usize,
    /// Request timeout in seconds.
    pub timeout_secs: u64,
}

impl Default for VisionConfig {
    fn default() -> Self {
        Self {
            ollama_url: "http://127.0.0.1:11434".to_string(),
            backend: EmbeddingBackend::ClipVitB32,
            max_image_bytes: 10 * 1024 * 1024,
            timeout_secs: 60,
        }
    }
}

/// The vision pipeline — embeds images via Ollama on GPU.
pub struct VisionPipeline {
    config: VisionConfig,
    client: reqwest::Client,
}

impl VisionPipeline {
    pub fn new(config: VisionConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .expect("failed to build HTTP client");
        Self { config, client }
    }

    pub fn with_backend(mut self, backend: EmbeddingBackend) -> Self {
        self.config.backend = backend;
        self
    }

    /// Embed an image from raw bytes.
    /// Sends base64-encoded image to Ollama for embedding extraction.
    #[instrument(skip(self, image_bytes), fields(backend = self.config.backend.model_tag()))]
    pub async fn embed_image(&self, image_bytes: &[u8]) -> Result<ImageEmbedding> {
        if image_bytes.len() > self.config.max_image_bytes {
            anyhow::bail!("Image too large: {} bytes (max: {})", image_bytes.len(), self.config.max_image_bytes);
        }

        let start = Instant::now();
        let b64 = BASE64.encode(image_bytes);
        let hash = format!("{:016x}", seahash::hash(image_bytes));

        let url = format!("{}/api/embeddings", self.config.ollama_url);
        let req_body = OllamaEmbedRequest {
            model: self.config.backend.model_tag().to_string(),
            input: b64,
        };

        let resp = self.client.post(&url)
            .json(&req_body)
            .send()
            .await?;

        if !resp.status().is_success() {
            anyhow::bail!("Ollama vision embedding failed: {}", resp.status());
        }

        let body: OllamaEmbedResponse = resp.json().await?;
        let vector = body.embeddings.into_iter().next()
            .ok_or_else(|| anyhow::anyhow!("No embedding returned from Ollama"))?;

        let dimension = vector.len();
        let duration_ms = start.elapsed().as_millis() as u64;

        info!("VisionPipeline: embedded image ({} bytes) in {}ms, dim={}", image_bytes.len(), duration_ms, dimension);

        Ok(ImageEmbedding {
            vector,
            dimension,
            backend: self.config.backend.clone(),
            image_hash: hash,
            duration_ms,
        })
    }

    /// Compute similarity between two images.
    pub async fn similarity(&self, img_a: &[u8], img_b: &[u8]) -> Result<f32> {
        let emb_a = self.embed_image(img_a).await?;
        let emb_b = self.embed_image(img_b).await?;
        Ok(emb_a.cosine_similarity(&emb_b))
    }

    /// Simple hash for image deduplication (no GPU needed).
    pub fn image_hash(image_bytes: &[u8]) -> String {
        format!("{:016x}", seahash::hash(image_bytes))
    }
}

/// Minimal seahash for image hashing (no external dep).
mod seahash {
    pub fn hash(data: &[u8]) -> u64 {
        let mut state: [u64; 4] = [0x16f11fe9b0da9bb7, 0x0959134254a2f0c3, 0x16f11fe9b0da9bb7, 0x0959134254a2f0c3];
        for &byte in data {
            state[0] = state[0].wrapping_mul(0x6eed0e9da499af27).wrapping_add(byte as u64);
            state[0] ^= state[0] >> 32;
        }
        state[0] ^ state[1] ^ state[2] ^ state[3]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vision_config_defaults() {
        let config = VisionConfig::default();
        assert_eq!(config.ollama_url, "http://127.0.0.1:11434");
        assert_eq!(config.backend, EmbeddingBackend::ClipVitB32);
        assert_eq!(config.max_image_bytes, 10 * 1024 * 1024);
    }

    #[test]
    fn backend_model_tags() {
        assert_eq!(EmbeddingBackend::ClipVitB32.model_tag(), "clip-vit-b-32");
        assert_eq!(EmbeddingBackend::Moondream2.model_tag(), "moondream2");
        assert_eq!(EmbeddingBackend::Llava.model_tag(), "llava");
        assert_eq!(EmbeddingBackend::Custom("my-model".into()).model_tag(), "my-model");
    }

    #[test]
    fn embedding_cosine_similarity() {
        let emb_a = ImageEmbedding {
            vector: vec![1.0, 0.0, 0.0],
            dimension: 3,
            backend: EmbeddingBackend::ClipVitB32,
            image_hash: "a".to_string(),
            duration_ms: 10,
        };
        let emb_b = ImageEmbedding {
            vector: vec![1.0, 0.0, 0.0],
            dimension: 3,
            backend: EmbeddingBackend::ClipVitB32,
            image_hash: "b".to_string(),
            duration_ms: 10,
        };
        // Identical vectors → similarity ≈ 1.0
        let sim = emb_a.cosine_similarity(&emb_b);
        assert!((sim - 1.0).abs() < 0.01, "expected ~1.0, got {}", sim);

        // Orthogonal
        let emb_c = ImageEmbedding {
            vector: vec![0.0, 1.0, 0.0],
            dimension: 3,
            backend: EmbeddingBackend::ClipVitB32,
            image_hash: "c".to_string(),
            duration_ms: 10,
        };
        let sim_orth = emb_a.cosine_similarity(&emb_c);
        assert!(sim_orth.abs() < 0.01, "expected ~0.0, got {}", sim_orth);
    }

    #[test]
    fn image_hash_deterministic() {
        let h1 = VisionPipeline::image_hash(&[1, 2, 3]);
        let h2 = VisionPipeline::image_hash(&[1, 2, 3]);
        assert_eq!(h1, h2);
        let h3 = VisionPipeline::image_hash(&[3, 2, 1]);
        assert_ne!(h1, h3);
    }

    #[test]
    fn dimension_mismatch_returns_zero() {
        let emb_a = ImageEmbedding {
            vector: vec![1.0, 0.0],
            dimension: 2,
            backend: EmbeddingBackend::ClipVitB32,
            image_hash: "a".to_string(),
            duration_ms: 10,
        };
        let emb_b = ImageEmbedding {
            vector: vec![1.0, 0.0, 0.0],
            dimension: 3,
            backend: EmbeddingBackend::ClipVitB32,
            image_hash: "b".to_string(),
            duration_ms: 10,
        };
        assert_eq!(emb_a.cosine_similarity(&emb_b), 0.0);
    }
}