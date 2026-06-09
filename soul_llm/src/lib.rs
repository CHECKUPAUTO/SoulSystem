use serde::{Deserialize, Serialize};
use thiserror::Error;
use std::time::Duration;

#[derive(Error, Debug)]
pub enum LlmError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Ollama not reachable at {0}")]
    NotReachable(String),
    #[error("Streaming error: {0}")]
    Stream(String),
    #[error("Timeout after {0}s")]
    Timeout(u64),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub base_url: String,
    pub model: String,
    pub temperature: f32,
    pub max_tokens: usize,
    pub system_prompt: Option<String>,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    #[serde(default = "default_retry_base_delay_ms")]
    pub retry_base_delay_ms: u64,
}

fn default_max_retries() -> u32 { 3 }

fn default_retry_base_delay_ms() -> u64 { 1000 }

fn default_timeout() -> u64 {
    120
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            base_url: "http://127.0.0.1:11434".to_string(),
            model: "qwen3:4b".to_string(),
            temperature: 0.7,
            max_tokens: 4096,
            system_prompt: None,
            timeout_secs: 120,
            max_retries: 3,
            retry_base_delay_ms: 1000,
        }
    }
}

impl LlmConfig {
    pub fn with_model(mut self, model: &str) -> Self {
        self.model = model.into();
        self
    }

    pub fn with_temperature(mut self, temp: f32) -> Self {
        self.temperature = temp;
        self
    }

    pub fn with_max_tokens(mut self, tokens: usize) -> Self {
        self.max_tokens = tokens;
        self
    }

    /// Select model based on task complexity (3-axis inference: capability tier)
    pub fn for_complexity(complexity: u8) -> Self {
        if complexity < 3 {
            Self::default().with_model("qwen3:4b").with_max_tokens(2048)
        } else if complexity < 6 {
            Self::default().with_model("qwen3:8b").with_max_tokens(4096)
        } else {
            Self::default().with_model("qwen3:8b").with_max_tokens(8192)
        }
    }
}

// ── Chat Messages ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: FunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

// ── Tool Schema ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    #[serde(rename = "type")]
    pub schema_type: String,
    pub function: FunctionSchema,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionSchema {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

// ── Ollama API Types ──────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct GenerateRequest {
    model: String,
    prompt: String,
    stream: bool,
    options: Option<GenerateOptions>,
}

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
    tools: Option<Vec<ToolSchema>>,
    options: Option<GenerateOptions>,
}

#[derive(Debug, Serialize)]
struct GenerateOptions {
    temperature: f32,
    num_predict: usize,
}

#[derive(Debug, Deserialize)]
pub struct GenerateResponse {
    pub response: String,
    pub done: bool,
    pub total_duration: Option<u64>,
    pub eval_count: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct ChatResponse {
    pub message: ChatResponseMessage,
    pub done: bool,
    pub total_duration: Option<u64>,
    pub eval_count: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct ChatResponseMessage {
    pub role: String,
    pub content: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Debug, Serialize)]
struct EmbedRequest {
    model: String,
    input: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct EmbedResponse {
    pub embeddings: Vec<Vec<f32>>,
}

#[derive(Debug, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    pub size: Option<u64>,
    pub parameter_size: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ModelsResponse {
    models: Vec<ModelInfo>,
}

// ── Streaming ──────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct StreamChunk {
    pub message: Option<StreamMessage>,
    pub done: bool,
}

#[derive(Debug, Deserialize)]
pub struct StreamMessage {
    pub role: Option<String>,
    pub content: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
}

// ── Ollama Client ─────────────────────────────────────────────────────

pub struct OllamaClient {
    config: LlmConfig,
    http: reqwest::Client,
}

impl OllamaClient {
    pub fn new(config: LlmConfig) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self { config, http }
    }

    pub async fn is_alive(&self) -> bool {
        self.http
            .get(format!("{}/api/tags", self.config.base_url))
            .send()
            .await
            .is_ok()
    }

    /// Execute an HTTP request with exponential backoff retry on transport errors.
    /// Does NOT retry on 4xx responses or JSON parse failures.
    async fn execute_with_retry<F, Fut, T>(&self, request_fn: F) -> Result<T, LlmError>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T, LlmError>>,
    {
        let mut last_error = None;
        let max_attempts = self.config.max_retries + 1;

        for attempt in 1..=max_attempts {
            match request_fn().await {
                Ok(val) => return Ok(val),
                Err(e) => {
                    let is_transport = matches!(&e, LlmError::Http(err) if err.is_timeout() || err.is_connect());
                    if !is_transport {
                        return Err(e);
                    }
                    if attempt == max_attempts {
                        return Err(e);
                    }
                    last_error = Some(e);
                    let delay_ms = self.config.retry_base_delay_ms
                        .saturating_mul(2u64.saturating_pow(attempt - 1))
                        .min(10_000);
                    tracing::warn!(
                        "LLM request failed (attempt {}/{}), retrying in {}ms",
                        attempt,
                        max_attempts,
                        delay_ms
                    );
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                }
            }
        }

        Err(last_error.unwrap_or_else(|| LlmError::NotReachable("max retries exceeded".into())))
    }

    pub async fn list_models(&self) -> Result<Vec<ModelInfo>, LlmError> {
        let resp: ModelsResponse = self
            .http
            .get(format!("{}/api/tags", self.config.base_url))
            .send()
            .await?
            .json()
            .await?;
        Ok(resp.models)
    }

    // ── Simple generate (no context) ──

    pub async fn generate(&self, prompt: &str) -> Result<GenerateResponse, LlmError> {
        let req = GenerateRequest {
            model: self.config.model.clone(),
            prompt: prompt.to_string(),
            stream: false,
            options: Some(GenerateOptions {
                temperature: self.config.temperature,
                num_predict: self.config.max_tokens,
            }),
        };

        self.execute_with_retry(|| async {
            Ok(self
                .http
                .post(format!("{}/api/generate", self.config.base_url))
                .json(&req)
                .send()
                .await?
                .json::<GenerateResponse>()
                .await?)
        })
        .await
    }

    // ── Chat with conversation context ──

    pub async fn chat(
        &self,
        messages: &[ChatMessage],
        tools: Option<&[ToolSchema]>,
    ) -> Result<ChatResponse, LlmError> {
        let req = ChatRequest {
            model: self.config.model.clone(),
            messages: messages.to_vec(),
            stream: false,
            tools: tools.map(|t| t.to_vec()),
            options: Some(GenerateOptions {
                temperature: self.config.temperature,
                num_predict: self.config.max_tokens,
            }),
        };

        self.execute_with_retry(|| async {
            Ok(self
                .http
                .post(format!("{}/api/chat", self.config.base_url))
                .json(&req)
                .send()
                .await?
                .json::<ChatResponse>()
                .await?)
        })
        .await
    }

    // ── Streaming chat ──

    pub async fn chat_stream<F>(
        &self,
        messages: &[ChatMessage],
        tools: Option<&[ToolSchema]>,
        mut on_chunk: F,
    ) -> Result<String, LlmError>
    where
        F: FnMut(&str) + Send,
    {
        use futures::StreamExt;
        use tokio::io::AsyncBufReadExt;

        let req = ChatRequest {
            model: self.config.model.clone(),
            messages: messages.to_vec(),
            stream: true,
            tools: tools.map(|t| t.to_vec()),
            options: Some(GenerateOptions {
                temperature: self.config.temperature,
                num_predict: self.config.max_tokens,
            }),
        };

        let resp = self
            .http
            .post(format!("{}/api/chat", self.config.base_url))
            .json(&req)
            .send()
            .await?;

        let mut full_response = String::new();
        let mut stream = resp.bytes_stream();
        let mut buffer = String::new();

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result.map_err(|e| LlmError::Stream(e.to_string()))?;
            let text = String::from_utf8_lossy(&chunk);
            buffer.push_str(&text);

            // Process complete lines
            while let Some(newline_pos) = buffer.find('\n') {
                let line = buffer[..newline_pos].trim().to_string();
                buffer = buffer[newline_pos + 1..].to_string();

                if line.is_empty() {
                    continue;
                }

                if let Ok(chunk) = serde_json::from_str::<StreamChunk>(&line) {
                    if let Some(msg) = chunk.message {
                        if let Some(content) = msg.content {
                            full_response.push_str(&content);
                            on_chunk(&content);
                        }
                    }
                    if chunk.done {
                        return Ok(full_response);
                    }
                }
            }
        }

        // Process any remaining buffer
        if !buffer.trim().is_empty() {
            if let Ok(chunk) = serde_json::from_str::<StreamChunk>(&buffer) {
                if let Some(msg) = chunk.message {
                    if let Some(content) = msg.content {
                        full_response.push_str(&content);
                        on_chunk(&content);
                    }
                }
            }
        }

        Ok(full_response)
    }

    // ── Embeddings ──

    pub async fn embed(&self, text: &str) -> Result<Vec<f32>, LlmError> {
        let req = EmbedRequest {
            model: self.config.model.clone(),
            input: vec![text.to_string()],
        };

        let resp: EmbedResponse = self
            .http
            .post(format!("{}/api/embed", self.config.base_url))
            .json(&req)
            .send()
            .await?
            .json()
            .await?;

        resp.embeddings
            .into_iter()
            .next()
            .ok_or(LlmError::NotReachable("no embedding returned".into()))
    }

    pub async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, LlmError> {
        let req = EmbedRequest {
            model: self.config.model.clone(),
            input: texts.to_vec(),
        };

        let resp: EmbedResponse = self
            .http
            .post(format!("{}/api/embed", self.config.base_url))
            .json(&req)
            .send()
            .await?
            .json()
            .await?;

        Ok(resp.embeddings)
    }

    pub fn config(&self) -> &LlmConfig {
        &self.config
    }
}

// Blocking versions for REPL and non-async contexts
pub struct OllamaClientBlocking {
    config: LlmConfig,
    http: reqwest::blocking::Client,
}

impl OllamaClientBlocking {
    pub fn new(config: LlmConfig) -> Self {
        Self {
            config,
            http: reqwest::blocking::Client::new(),
        }
    }

    pub fn is_alive(&self) -> bool {
        self.http
            .get(format!("{}/api/tags", self.config.base_url))
            .send()
            .is_ok()
    }

    pub fn list_models(&self) -> Result<Vec<ModelInfo>, LlmError> {
        let resp: ModelsResponse = self
            .http
            .get(format!("{}/api/tags", self.config.base_url))
            .send()?
            .json()?;
        Ok(resp.models)
    }

    pub fn generate(&self, prompt: &str) -> Result<GenerateResponse, LlmError> {
        let req = GenerateRequest {
            model: self.config.model.clone(),
            prompt: prompt.to_string(),
            stream: false,
            options: Some(GenerateOptions {
                temperature: self.config.temperature,
                num_predict: self.config.max_tokens,
            }),
        };

        let resp: GenerateResponse = self
            .http
            .post(format!("{}/api/generate", self.config.base_url))
            .json(&req)
            .send()?
            .json()?;

        Ok(resp)
    }

    pub fn config(&self) -> &LlmConfig {
        &self.config
    }
}
