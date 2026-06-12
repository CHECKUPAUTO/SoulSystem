use crate::cache::{CacheConfig, CacheStats, LlmCache};
use crate::config::LlmConfig;
use anyhow::{Context, Result};
use rand::Rng;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Serialize)]
struct OllamaRequest<'a> {
    model: &'a str,
    prompt: &'a str,
    stream: bool,
    options: OllamaOptions,
}

#[derive(Debug, Serialize)]
struct OllamaOptions {
    temperature: f64,
    num_predict: u32,
}

#[derive(Debug, Deserialize)]
struct OllamaResponse {
    response: String,
}

/// Client LLM — Ollama avec sélection pondérée primaire/secondaire.
#[derive(Clone)]
pub struct LlmClient {
    cfg: LlmConfig,
    http: Client,
    /// Optional response cache. Set via `with_cache(...)`. `None` disables it.
    cache: Option<Arc<LlmCache>>,
}

impl LlmClient {
    pub fn new(cfg: LlmConfig) -> Result<Self> {
        let http = Client::builder()
            .timeout(Duration::from_secs(cfg.timeout_secs))
            .build()
            .context("Création client HTTP")?;
        Ok(Self {
            cfg,
            http,
            cache: None,
        })
    }

    /// Plug a shared response cache. Returns a new client; old clones are unaffected.
    pub fn with_cache(mut self, cache: Arc<LlmCache>) -> Self {
        self.cache = Some(cache);
        self
    }

    /// Expose cache stats (or zeros if no cache).
    pub fn cache_stats(&self) -> CacheStats {
        self.cache.as_ref().map(|c| c.stats()).unwrap_or_default()
    }

    /// Build a cache instance from a CacheConfig, ready to plug via `with_cache`.
    pub fn make_cache(cfg: CacheConfig) -> Arc<LlmCache> {
        LlmCache::shared(cfg)
    }

    fn select_model(&self) -> &str {
        if rand::thread_rng().gen::<f64>() < self.cfg.primary_weight {
            &self.cfg.primary_model
        } else {
            &self.cfg.secondary_model
        }
    }

    async fn call(
        &self,
        model: &str,
        prompt: &str,
        temperature: f64,
        max_tokens: u32,
    ) -> Result<String> {
        let body = OllamaRequest {
            model,
            prompt,
            stream: false,
            options: OllamaOptions {
                temperature,
                num_predict: max_tokens,
            },
        };
        let url = format!("{}/generate", self.cfg.api_base);
        let resp = self
            .http
            .post(&url)
            .json(&body)
            .send()
            .await
            .with_context(|| format!("Requête Ollama vers {url}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Ollama HTTP {status}: {body}");
        }

        let r: OllamaResponse = resp.json().await.context("Désérialisation Ollama")?;
        Ok(r.response.trim().to_string())
    }

    /// Mutation via un prompt déjà construit par PromptBuilder.
    pub async fn mutate_with_prompt(&self, prompt: &str) -> Result<String> {
        let model = self.select_model().to_string();
        let temp = self.cfg.temperature as f32;
        if let Some(cache) = &self.cache {
            if let Some(hit) = cache.get("ollama", &model, temp, prompt) {
                return Ok(hit);
            }
        }
        let resp = self
            .call(&model, prompt, self.cfg.temperature, self.cfg.max_tokens)
            .await?;
        if let Some(cache) = &self.cache {
            cache.put("ollama", &model, temp, prompt, resp.clone());
        }
        Ok(resp)
    }

    /// Mutation simple (rétro-compatibilité).
    pub async fn mutate(&self, code: &str, system_hint: &str) -> Result<String> {
        let prompt = format!(
            "{system_hint}\n\n## Code\n```rust\n{code}\n```\n\n\
             Retourne uniquement le code Rust amélioré, sans markdown."
        );
        self.mutate_with_prompt(&prompt).await
    }

    /// Feedback du modèle secondaire sur un programme évalué.
    pub async fn feedback(
        &self,
        code: &str,
        score: f64,
        metrics: &HashMap<String, f64>,
    ) -> Result<String> {
        let prompt = crate::prompt::PromptBuilder::build_feedback_prompt(code, score, metrics);
        self.call(&self.cfg.secondary_model.clone(), &prompt, 0.4, 512)
            .await
    }

    /// Feedback via un prompt déjà construit (rétro-compatibilité).
    pub async fn feedback_with_prompt(&self, prompt: &str) -> Result<String> {
        self.call(&self.cfg.secondary_model.clone(), prompt, 0.4, 512)
            .await
    }
}
