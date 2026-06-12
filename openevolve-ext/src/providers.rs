//! Multi-provider LLM abstraction.
//!
//! Each provider implements the `LlmProvider` trait with a uniform `chat()`
//! shape, a `health_check()`, and a `name()` for logging. `chat_with_retry`
//! wraps any provider with exponential backoff + jitter.
//!
//! Providers:
//!   - `OllamaProvider`    — local first-class
//!   - `OpenAiProvider`    — OpenAI-compatible REST (also works with vLLM, OpenRouter, …)
//!   - `AnthropicProvider` — Anthropic Messages API
//!   - `GeminiProvider`    — Google Generative Language API
//!
//! Use `create_provider(&cfg)` to materialize an `Arc<dyn LlmProvider>` from a
//! `ProviderConfig`, then call `chat_with_retry(...)` for resilient invocation.

use anyhow::{Context, Result};
use async_trait::async_trait;
use rand::Rng;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, warn};

// ── Types ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// "ollama" | "openai" | "anthropic" | "gemini"
    pub provider: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub model: String,
    pub temperature: f32,
    pub max_tokens: u32,
    pub timeout_secs: u64,
    pub retries: u32,
    pub retry_delay_ms: u64,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            provider: "ollama".into(),
            base_url: "http://localhost:11434".into(),
            api_key: None,
            model: "qwen3-coder-next:cloud".into(),
            temperature: 0.8,
            max_tokens: 4096,
            timeout_secs: 120,
            retries: 3,
            retry_delay_ms: 500,
        }
    }
}

impl From<&crate::config::LlmConfig> for ProviderConfig {
    fn from(cfg: &crate::config::LlmConfig) -> Self {
        Self {
            provider: "ollama".into(),
            base_url: cfg.api_base.trim_end_matches("/api").to_string(),
            api_key: None,
            model: cfg.primary_model.clone(),
            temperature: cfg.temperature as f32,
            max_tokens: cfg.max_tokens,
            timeout_secs: cfg.timeout_secs,
            retries: 3,
            retry_delay_ms: 500,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn chat(
        &self,
        messages: &[ChatMessage],
        temperature: f32,
        max_tokens: u32,
    ) -> Result<String>;

    async fn health_check(&self) -> bool;

    fn name(&self) -> &str;
}

// ── Retry wrapper ─────────────────────────────────────────────

pub async fn chat_with_retry(
    provider: &dyn LlmProvider,
    messages: &[ChatMessage],
    temperature: f32,
    max_tokens: u32,
    retries: u32,
    base_delay_ms: u64,
) -> Result<String> {
    let base = Duration::from_millis(base_delay_ms);
    let mut last_err: Option<anyhow::Error> = None;
    for attempt in 0..=retries {
        match provider.chat(messages, temperature, max_tokens).await {
            Ok(s) if !s.trim().is_empty() => return Ok(s),
            Ok(_) => {
                warn!(attempt, provider = provider.name(), "empty response");
                last_err = Some(anyhow::anyhow!("empty response from {}", provider.name()));
            }
            Err(e) => {
                warn!(attempt, provider = provider.name(), error=%e, "request failed");
                last_err = Some(e);
            }
        }
        if attempt == retries {
            break;
        }
        let jitter: f64 = rand::thread_rng().gen_range(0.75..1.25);
        let delay = base.saturating_mul(2u32.pow(attempt));
        let delay = Duration::from_secs_f64(delay.as_secs_f64() * jitter);
        debug!(?delay, "retrying");
        tokio::time::sleep(delay).await;
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("all retries failed")))
        .with_context(|| format!("chat_with_retry exhausted {} attempts", retries + 1))
}

// ── Ollama ────────────────────────────────────────────────────

pub struct OllamaProvider {
    client: Client,
    base_url: String,
    model: String,
}

impl OllamaProvider {
    pub fn new(cfg: &ProviderConfig) -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(cfg.timeout_secs))
                .build()
                .expect("build reqwest client"),
            base_url: cfg.base_url.trim_end_matches('/').to_string(),
            model: cfg.model.clone(),
        }
    }
}

#[async_trait]
impl LlmProvider for OllamaProvider {
    async fn chat(
        &self,
        messages: &[ChatMessage],
        temperature: f32,
        max_tokens: u32,
    ) -> Result<String> {
        #[derive(Serialize)]
        struct Req<'a> {
            model: &'a str,
            messages: &'a [ChatMessage],
            stream: bool,
            options: Opts,
        }
        #[derive(Serialize)]
        struct Opts {
            temperature: f32,
            num_predict: u32,
        }
        #[derive(Deserialize)]
        struct Resp {
            message: ChatMessage,
        }

        let req = Req {
            model: &self.model,
            messages,
            stream: false,
            options: Opts {
                temperature,
                num_predict: max_tokens,
            },
        };
        let resp = self
            .client
            .post(format!("{}/api/chat", self.base_url))
            .json(&req)
            .send()
            .await
            .context("ollama request failed")?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Ollama HTTP {status}: {}", truncate(&body, 400));
        }
        let r: Resp = resp.json().await.context("parse ollama response")?;
        Ok(r.message.content)
    }

    async fn health_check(&self) -> bool {
        self.client
            .get(format!("{}/api/version", self.base_url))
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    fn name(&self) -> &str {
        "ollama"
    }
}

// ── OpenAI ────────────────────────────────────────────────────

pub struct OpenAiProvider {
    client: Client,
    base_url: String,
    api_key: String,
    model: String,
}

impl OpenAiProvider {
    pub fn new(cfg: &ProviderConfig) -> Self {
        let base = if cfg.base_url.contains("localhost") || cfg.base_url.contains("127.0.0.1") {
            "https://api.openai.com/v1".to_string()
        } else {
            cfg.base_url.trim_end_matches('/').to_string()
        };
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(cfg.timeout_secs))
                .build()
                .expect("build reqwest client"),
            base_url: base,
            api_key: cfg.api_key.clone().unwrap_or_default(),
            model: cfg.model.clone(),
        }
    }
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    async fn chat(
        &self,
        messages: &[ChatMessage],
        temperature: f32,
        max_tokens: u32,
    ) -> Result<String> {
        #[derive(Serialize)]
        struct Req<'a> {
            model: &'a str,
            messages: &'a [ChatMessage],
            temperature: f32,
            max_tokens: u32,
        }
        #[derive(Deserialize)]
        struct Resp {
            choices: Vec<Choice>,
        }
        #[derive(Deserialize)]
        struct Choice {
            message: ChatMessage,
        }

        let req = Req {
            model: &self.model,
            messages,
            temperature,
            max_tokens,
        };
        let resp = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&req)
            .send()
            .await
            .context("openai request failed")?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("OpenAI HTTP {status}: {}", truncate(&body, 400));
        }
        let chat: Resp = resp.json().await.context("parse openai response")?;
        chat.choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .context("no choices in openai response")
    }

    async fn health_check(&self) -> bool {
        if self.api_key.is_empty() {
            return false;
        }
        self.client
            .get(format!("{}/models", self.base_url))
            .bearer_auth(&self.api_key)
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    fn name(&self) -> &str {
        "openai"
    }
}

// ── Anthropic ─────────────────────────────────────────────────

pub struct AnthropicProvider {
    client: Client,
    base_url: String,
    api_key: String,
    model: String,
}

impl AnthropicProvider {
    pub fn new(cfg: &ProviderConfig) -> Self {
        let base = if cfg.base_url.contains("localhost") || cfg.base_url.contains("127.0.0.1") {
            "https://api.anthropic.com/v1".to_string()
        } else {
            cfg.base_url.trim_end_matches('/').to_string()
        };
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(cfg.timeout_secs))
                .build()
                .expect("build reqwest client"),
            base_url: base,
            api_key: cfg.api_key.clone().unwrap_or_default(),
            model: cfg.model.clone(),
        }
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    async fn chat(
        &self,
        messages: &[ChatMessage],
        temperature: f32,
        max_tokens: u32,
    ) -> Result<String> {
        #[derive(Serialize)]
        struct Req<'a> {
            model: &'a str,
            max_tokens: u32,
            temperature: f32,
            messages: &'a [ChatMessage],
        }
        #[derive(Deserialize)]
        struct Resp {
            content: Vec<Block>,
        }
        #[derive(Deserialize)]
        struct Block {
            #[serde(default)]
            text: String,
        }

        let req = Req {
            model: &self.model,
            max_tokens,
            temperature,
            messages,
        };
        let resp = self
            .client
            .post(format!("{}/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&req)
            .send()
            .await
            .context("anthropic request failed")?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Anthropic HTTP {status}: {}", truncate(&body, 400));
        }
        let chat: Resp = resp.json().await.context("parse anthropic response")?;
        Ok(chat
            .content
            .into_iter()
            .map(|c| c.text)
            .collect::<Vec<_>>()
            .join(""))
    }

    async fn health_check(&self) -> bool {
        !self.api_key.is_empty()
    }

    fn name(&self) -> &str {
        "anthropic"
    }
}

// ── Gemini ────────────────────────────────────────────────────

pub struct GeminiProvider {
    client: Client,
    base_url: String,
    api_key: String,
    model: String,
}

impl GeminiProvider {
    pub fn new(cfg: &ProviderConfig) -> Self {
        let base = if cfg.base_url.contains("localhost") || cfg.base_url.contains("127.0.0.1") {
            "https://generativelanguage.googleapis.com/v1beta".to_string()
        } else {
            cfg.base_url.trim_end_matches('/').to_string()
        };
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(cfg.timeout_secs))
                .build()
                .expect("build reqwest client"),
            base_url: base,
            api_key: cfg.api_key.clone().unwrap_or_default(),
            model: cfg.model.clone(),
        }
    }
}

#[async_trait]
impl LlmProvider for GeminiProvider {
    async fn chat(
        &self,
        messages: &[ChatMessage],
        temperature: f32,
        max_tokens: u32,
    ) -> Result<String> {
        #[derive(Serialize)]
        struct Req<'a> {
            contents: Vec<Content<'a>>,
            #[serde(rename = "generationConfig")]
            generation_config: GenCfg,
        }
        #[derive(Serialize)]
        struct Content<'a> {
            role: &'a str,
            parts: Vec<Part<'a>>,
        }
        #[derive(Serialize)]
        struct Part<'a> {
            text: &'a str,
        }
        #[derive(Serialize)]
        struct GenCfg {
            temperature: f32,
            #[serde(rename = "maxOutputTokens")]
            max_output_tokens: u32,
        }
        #[derive(Deserialize)]
        struct Resp {
            candidates: Vec<Candidate>,
        }
        #[derive(Deserialize)]
        struct Candidate {
            content: CRet,
        }
        #[derive(Deserialize)]
        struct CRet {
            parts: Vec<PRet>,
        }
        #[derive(Deserialize)]
        struct PRet {
            #[serde(default)]
            text: String,
        }

        let contents: Vec<Content> = messages
            .iter()
            .map(|m| Content {
                role: if m.role == "assistant" {
                    "model"
                } else {
                    "user"
                },
                parts: vec![Part { text: &m.content }],
            })
            .collect();
        let req = Req {
            contents,
            generation_config: GenCfg {
                temperature,
                max_output_tokens: max_tokens,
            },
        };
        let resp = self
            .client
            .post(format!(
                "{}/models/{}:generateContent?key={}",
                self.base_url, self.model, self.api_key
            ))
            .json(&req)
            .send()
            .await
            .context("gemini request failed")?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Gemini HTTP {status}: {}", truncate(&body, 400));
        }
        let chat: Resp = resp.json().await.context("parse gemini response")?;
        chat.candidates
            .into_iter()
            .next()
            .and_then(|c| c.content.parts.into_iter().next())
            .map(|p| p.text)
            .context("no content in gemini response")
    }

    async fn health_check(&self) -> bool {
        !self.api_key.is_empty()
    }

    fn name(&self) -> &str {
        "gemini"
    }
}

// ── Factory ───────────────────────────────────────────────────

pub fn create_provider(cfg: &ProviderConfig) -> Arc<dyn LlmProvider> {
    match cfg.provider.as_str() {
        "openai" => Arc::new(OpenAiProvider::new(cfg)),
        "anthropic" => Arc::new(AnthropicProvider::new(cfg)),
        "gemini" => Arc::new(GeminiProvider::new(cfg)),
        _ => Arc::new(OllamaProvider::new(cfg)),
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        let mut end = max;
        while !s.is_char_boundary(end) && end > 0 {
            end -= 1;
        }
        format!("{}…", &s[..end])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_default() {
        let cfg = ProviderConfig::default();
        assert_eq!(cfg.provider, "ollama");
    }

    #[test]
    fn factory_picks_right_impl() {
        let cfg = ProviderConfig {
            provider: "anthropic".into(),
            api_key: Some("test".into()),
            ..Default::default()
        };
        let p = create_provider(&cfg);
        assert_eq!(p.name(), "anthropic");
    }
}
