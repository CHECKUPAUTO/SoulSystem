//! Ollama embedding client — calls /api/embeddings.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct OllamaEmbeddingClient {
    client: reqwest::Client,
    base_url: String,
    model: String,
}

#[derive(Deserialize)]
struct EmbedResponse {
    embedding: Vec<f32>,
}

impl OllamaEmbeddingClient {
    pub fn new(base_url: &str, model: &str) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("failed to build reqwest client");
        Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            model: model.to_string(),
        }
    }

    /// Get embedding vector for a text string.
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let url = format!("{}/api/embeddings", self.base_url);
        let body = serde_json::json!({
            "model": self.model,
            "prompt": text,
        });
        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .context("sending embedding request to Ollama")?;
        let data: EmbedResponse = resp
            .json()
            .await
            .context("parsing Ollama embedding response")?;
        Ok(data.embedding)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_construction() {
        let c = OllamaEmbeddingClient::new("http://localhost:11434", "nomic-embed-text");
        assert_eq!(c.base_url, "http://localhost:11434");
        assert_eq!(c.model, "nomic-embed-text");
    }

    #[test]
    fn client_trims_trailing_slash() {
        let c = OllamaEmbeddingClient::new("http://localhost:11434/", "test");
        assert_eq!(c.base_url, "http://localhost:11434");
    }
}
