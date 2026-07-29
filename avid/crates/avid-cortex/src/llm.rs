use async_trait::async_trait;
use std::time::Duration;
use soulsystem_common::secrets::SecretString;

/// Abstract trait for LLM providers.
#[async_trait]
pub trait LlmProvider: Send + Sync + std::fmt::Debug {
    /// Send a completion request to the provider.
    async fn complete(&self, system: &str, user: &str, timeout: Duration) -> Result<String, anyhow::Error>;
    /// Get the provider name.
    fn name(&self) -> &str;
}

/// Ollama LLM provider implementation.
#[derive(Debug)]
pub struct OllamaProvider {
    pub model: String,
    pub base_url: String,
}

#[async_trait]
impl LlmProvider for OllamaProvider {
    async fn complete(&self, system: &str, user: &str, timeout: Duration) -> Result<String, anyhow::Error> {
        let client = reqwest::Client::builder().timeout(timeout).build()?;
        let response = client.post(format!("{}/api/chat", self.base_url))
            .json(&serde_json::json!({
                "model": self.model,
                "messages": [{"role": "system", "content": system}, {"role": "user", "content": user}],
                "stream": false
            })).send().await?.json::<serde_json::Value>().await?;
        let content = response["message"]["content"].as_str().ok_or_else(|| anyhow::anyhow!("Invalid response"))?;
        Ok(content.to_string())
    }
    fn name(&self) -> &str { "ollama" }
}

/// OpenAI LLM provider implementation.
#[derive(Debug)]
pub struct OpenAIProvider {
    pub api_key: SecretString,
    pub model: String,
}

#[async_trait]
impl LlmProvider for OpenAIProvider {
    async fn complete(&self, system: &str, user: &str, timeout: Duration) -> Result<String, anyhow::Error> {
        let client = reqwest::Client::builder().timeout(timeout).build()?;
        let response = client.post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key.expose()))
            .json(&serde_json::json!({
                "model": self.model,
                "messages": [{"role": "system", "content": system}, {"role": "user", "content": user}]
            })).send().await?.json::<serde_json::Value>().await?;
        let content = response["choices"][0]["message"]["content"].as_str().ok_or_else(|| anyhow::anyhow!("Invalid response"))?;
        Ok(content.to_string())
    }
    fn name(&self) -> &str { "openai" }
}

#[derive(Debug)]
pub struct AnthropicProvider { pub api_key: SecretString, pub model: String }
#[async_trait]
impl LlmProvider for AnthropicProvider {
    async fn complete(&self, system: &str, user: &str, timeout: Duration) -> Result<String, anyhow::Error> {
        let client = reqwest::Client::builder().timeout(timeout).build()?;
        let response = client.post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", self.api_key.expose()).header("anthropic-version", "2023-06-01")
            .json(&serde_json::json!({
                "model": self.model, "system": system, "messages": [{"role": "user", "content": user}], "max_tokens": 1024
            })).send().await?.json::<serde_json::Value>().await?;
        let content = response["content"][0]["text"].as_str().ok_or_else(|| anyhow::anyhow!("Invalid response"))?;
        Ok(content.to_string())
    }
    fn name(&self) -> &str { "anthropic" }
}

#[derive(Debug)]
pub struct GeminiProvider { pub api_key: SecretString, pub model: String }
#[async_trait]
impl LlmProvider for GeminiProvider {
    async fn complete(&self, _system: &str, user: &str, timeout: Duration) -> Result<String, anyhow::Error> {
        let client = reqwest::Client::builder().timeout(timeout).build()?;
        let url = format!("https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}", self.model, self.api_key.expose());
        let response = client.post(url).json(&serde_json::json!({ "contents": [{"parts": [{"text": user}]}] })).send().await?.json::<serde_json::Value>().await?;
        let content = response["candidates"][0]["content"]["parts"][0]["text"].as_str().ok_or_else(|| anyhow::anyhow!("Invalid response"))?;
        Ok(content.to_string())
    }
    fn name(&self) -> &str { "gemini" }
}
