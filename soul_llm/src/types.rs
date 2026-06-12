use serde::{Deserialize, Serialize};

// ── Provider kind ────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    Ollama,
    OpenAI,
    Anthropic,
}

impl Default for ProviderKind {
    fn default() -> Self {
        Self::Ollama
    }
}

impl std::fmt::Display for ProviderKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ollama => write!(f, "ollama"),
            Self::OpenAI => write!(f, "openai"),
            Self::Anthropic => write!(f, "anthropic"),
        }
    }
}

impl std::str::FromStr for ProviderKind {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "ollama" => Ok(Self::Ollama),
            "openai" => Ok(Self::OpenAI),
            "anthropic" => Ok(Self::Anthropic),
            _ => Err(format!("unknown provider: {s}")),
        }
    }
}

// ── Token usage ──────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
}

impl TokenUsage {
    pub fn new(prompt_tokens: usize, completion_tokens: usize) -> Self {
        Self {
            prompt_tokens,
            completion_tokens,
            total_tokens: prompt_tokens + completion_tokens,
        }
    }
}

// ── Generate request (agnostique) ────────────────────────────

#[derive(Debug, Clone)]
pub struct GenerateRequest {
    pub prompt: String,
    pub model: Option<String>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<usize>,
    pub stream: bool,
    pub system: Option<String>,
}

impl GenerateRequest {
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            model: None,
            temperature: None,
            max_tokens: None,
            stream: false,
            system: None,
        }
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    pub fn with_temperature(mut self, temp: f32) -> Self {
        self.temperature = Some(temp);
        self
    }

    pub fn with_max_tokens(mut self, n: usize) -> Self {
        self.max_tokens = Some(n);
        self
    }

    pub fn with_stream(mut self, stream: bool) -> Self {
        self.stream = stream;
        self
    }

    pub fn with_system(mut self, system: impl Into<String>) -> Self {
        self.system = Some(system.into());
        self
    }
}

// ── Generate result (agnostique) ─────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateResult {
    pub text: String,
    pub usage: TokenUsage,
    pub model: String,
    pub duration_ms: u64,
}

// ── Stream chunk ─────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct StreamChunk {
    pub text: String,
    pub done: bool,
    pub usage: Option<TokenUsage>,
}

// ── Embedding result ─────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct EmbeddingResult {
    pub embedding: Vec<f32>,
    pub model: String,
    pub usage: Option<TokenUsage>,
}

// ── Model info ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    pub size: Option<u64>,
    pub family: Option<String>,
}

// ── LlmConfig ────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct LlmConfig {
    pub provider: ProviderKind,
    pub base_url: String,
    pub model: String,
    pub temperature: f32,
    pub max_tokens: usize,
    pub http_timeout: Duration,
    pub connect_timeout: Duration,
    pub auth_token: Option<String>,
    pub goal_token_budget: usize,
    pub tokens_per_minute_budget: usize,
    pub pool_max_idle: usize,
    pub pool_idle_timeout: Duration,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: ProviderKind::Ollama,
            base_url: "http://127.0.0.1:11434".into(),
            model: "qwen3:8b".into(),
            temperature: 0.7,
            max_tokens: 2048,
            http_timeout: Duration::from_secs(30),
            connect_timeout: Duration::from_secs(5),
            auth_token: None,
            goal_token_budget: 50000,
            tokens_per_minute_budget: 100000,
            pool_max_idle: 10,
            pool_idle_timeout: Duration::from_secs(30),
        }
    }
}

use std::time::Duration;
