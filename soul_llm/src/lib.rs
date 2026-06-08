//! # soul_llm — Client HTTP pur Rust vers Ollama
//! Le cerveau de SoulSystem. Aucune dépendance Python.

use serde::{Deserialize, Serialize};

// ── Config ───────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct LlmConfig {
    pub base_url: String,
    pub model: String,
    pub temperature: f32,
    pub max_tokens: usize,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            base_url: "http://127.0.0.1:11434".into(),
            model: "qwen3:8b".into(),
            temperature: 0.7,
            max_tokens: 2048,
        }
    }
}

// ── Types Ollama ─────────────────────────────────────────────

#[derive(Serialize)]
struct GenerateRequest<'a> {
    model: &'a str,
    prompt: String,
    stream: bool,
}

#[derive(Deserialize)]
pub struct GenerateResponse {
    pub response: String,
    #[allow(dead_code)]
    pub done: bool,
}

#[derive(Serialize)]
struct EmbedRequest<'a> {
    model: &'a str,
    input: String,
}

#[derive(Deserialize)]
pub struct EmbedResponse {
    pub embedding: Vec<f32>,
}

// ── Client ───────────────────────────────────────────────────

pub struct OllamaClient {
    config: LlmConfig,
    http: reqwest::Client,
}

impl OllamaClient {
    pub fn new(config: LlmConfig) -> Self {
        Self {
            config,
            http: reqwest::Client::new(),
        }
    }

    pub async fn is_alive(&self) -> bool {
        self.http
            .get(format!("{}/api/tags", self.config.base_url))
            .send()
            .await
            .is_ok()
    }

    pub async fn list_models(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let resp = self
            .http
            .get(format!("{}/api/tags", self.config.base_url))
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;

        let models: Vec<String> = if let Some(arr) = resp["models"].as_array() {
            arr.iter()
                .filter_map(|m| m["name"].as_str().map(String::from))
                .collect()
        } else {
            vec![]
        };

        Ok(models)
    }

    pub async fn generate(
        &self,
        prompt: &str,
    ) -> Result<GenerateResponse, Box<dyn std::error::Error>> {
        let req = GenerateRequest {
            model: &self.config.model,
            prompt: prompt.into(),
            stream: false,
        };

        let url = format!("{}/api/generate", self.config.base_url);
        let resp: GenerateResponse = self
            .http
            .post(&url)
            .json(&req)
            .send()
            .await?
            .json()
            .await?;

        Ok(resp)
    }

    pub async fn embed(&self, text: &str) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
        let req = EmbedRequest {
            model: &self.config.model,
            input: text.into(),
        };

        let url = format!("{}/api/embed", self.config.base_url);
        let resp: EmbedResponse = self
            .http
            .post(&url)
            .json(&req)
            .send()
            .await?
            .json()
            .await?;

        Ok(resp.embedding)
    }

    pub async fn embed_batch(
        &self,
        texts: &[String],
    ) -> Result<Vec<Vec<f32>>, Box<dyn std::error::Error>> {
        let mut results = Vec::with_capacity(texts.len());
        for text in texts {
            results.push(self.embed(text).await?);
        }
        Ok(results)
    }
}
