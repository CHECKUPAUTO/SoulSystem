//! Provider trait + shared types for LLM provider abstraction.
//!
//! Each provider (Ollama, OpenAI, etc.) implements this trait. The
//! provider registry discovers configured providers and routes requests.

pub mod config;
pub mod ollama;
pub mod openai;
pub mod registry;

use async_trait::async_trait;
use futures::stream::BoxStream;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A single message in a chat conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// Request body for `/api/chat`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    /// Provider-qualified model ID: `{provider}/{model_id}` or just `{model_id}`.
    pub model: Option<String>,
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub stream: bool,
    #[serde(default = "default_temperature")]
    pub temperature: f64,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
}

fn default_temperature() -> f64 {
    0.7
}
fn default_max_tokens() -> u32 {
    4096
}

/// Response from `/api/chat` (non-streaming).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub model: String,
    pub message: ChatMessage,
    pub usage: Option<TokenUsage>,
}

/// A single delta chunk in a streaming response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatDelta {
    pub content: String,
    pub finish_reason: Option<String>,
}

/// Token usage statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Request body for `/api/completion`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub prompt: String,
    pub model: Option<String>,
    #[serde(default = "default_temperature")]
    pub temperature: f64,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
}

/// Response from `/api/completion`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    pub model: String,
    pub text: String,
    pub usage: Option<TokenUsage>,
}

/// Request body for `/api/tools/call`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRequest {
    pub tool: String,
    pub args: serde_json::Value,
    #[serde(default)]
    pub model: Option<String>,
}

/// Response from `/api/tools/call`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallResponse {
    pub result: serde_json::Value,
}

/// Provider error type.
#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("provider not found: {0}")]
    NotFound(String),

    #[error("model not available: {0}")]
    ModelNotAvailable(String),

    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("provider returned error: {status} — {body}")]
    Upstream { status: u16, body: String },

    #[error("timeout after {0}s")]
    Timeout(u64),

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("circuit breaker open for provider: {0}")]
    CircuitOpen(String),
}

/// LLM provider abstraction.
#[async_trait]
pub trait Provider: Send + Sync {
    fn name(&self) -> &str;
    fn config(&self) -> &ProviderConfig;

    /// Non-streaming chat completion.
    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse, ProviderError>;

    /// Streaming chat completion (SSE).
    async fn chat_stream(
        &self,
        req: ChatRequest,
    ) -> Result<BoxStream<'static, Result<ChatDelta, ProviderError>>, ProviderError>;

    /// Simple text completion.
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, ProviderError>;
}

/// Provider configuration from TOML.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct ProviderConfig {
    pub provider_type: String,
    pub base_url: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default)]
    pub default_model: Option<String>,
    #[serde(default = "default_provider_timeout")]
    pub timeout_secs: u64,

    // ── Optional cost-aware routing metadata (all default to safe values, so
    //    existing configs keep working unchanged). Consumed by the learned
    //    router in `crate::routing`. ──────────────────────────────────────
    /// Estimated cost per 1k tokens in USD (0.0 for free/local models).
    #[serde(default)]
    pub cost_per_1k_tokens: f64,
    /// Average round-trip latency in milliseconds.
    #[serde(default)]
    pub avg_latency_ms: u64,
    /// Maximum context window in tokens (quality proxy).
    #[serde(default = "default_routing_max_tokens")]
    pub max_tokens: u64,
    /// Capabilities this provider serves (kebab/snake/alias forms accepted).
    #[serde(default)]
    pub capabilities: Vec<String>,
}

fn default_provider_timeout() -> u64 {
    30
}

fn default_routing_max_tokens() -> u64 {
    8192
}
