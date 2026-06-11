use serde::{Deserialize, Serialize};
use thiserror::Error;
use std::time::Duration;
use soullink_circuit::{CircuitBreaker, CircuitBreakerConfig, CircuitBreakerError, CircuitState};

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
    pub fallback_models: Vec<String>,
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
            base_url: std::env::var("OLLAMA_URL")
                .or_else(|_| std::env::var("OLLAMA_HOST"))
                .unwrap_or_else(|_| "http://127.0.0.1:11434".to_string()),
            model: std::env::var("OLLAMA_MODEL")
                .unwrap_or_else(|_| "qwen3:8b".to_string()),
            fallback_models: std::env::var("OLLAMA_FALLBACK_MODELS")
                .unwrap_or_else(|_| "tinyllama:latest,codellama:7b".to_string())
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
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

    pub fn with_fallback_models(mut self, models: Vec<String>) -> Self {
        self.fallback_models = models;
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
    circuit_breaker: CircuitBreaker,
}

impl OllamaClient {
    pub fn new(config: LlmConfig) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        let circuit_config = CircuitBreakerConfig::llm_provider("ollama");
        let circuit_breaker = CircuitBreaker::new("ollama-llm", circuit_config);

        Self { config, http, circuit_breaker }
    }

    pub async fn is_alive(&self) -> bool {
        self.http
            .get(format!("{}/api/tags", self.config.base_url))
            .send()
            .await
            .is_ok()
    }

    /// Get the current circuit breaker state
    pub async fn circuit_state(&self) -> CircuitState {
        self.circuit_breaker.state().await
    }

    /// Get circuit breaker stats
    pub async fn circuit_stats(&self) -> soullink_circuit::CircuitStats {
        self.circuit_breaker.stats().await
    }

    /// Manually record a success (for admin/recovery)
    pub async fn record_circuit_success(&self) {
        self.circuit_breaker.record_success().await;
    }

    /// Manually record a failure (for testing)
    pub async fn record_circuit_failure(&self) {
        self.circuit_breaker.record_failure().await;
    }

    /// Execute an HTTP request with circuit breaker + exponential backoff retry on transport errors.
    /// Does NOT retry on 4xx responses or JSON parse failures.
    async fn execute_with_retry<F, Fut, T>(&self, request_fn: F) -> Result<T, LlmError>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T, LlmError>>,
    {
        // Use circuit breaker to execute with protection
        let result = self
            .circuit_breaker
            .call(|| async {
                let mut last_error = None;
                let max_attempts = self.config.max_retries + 1;

                for attempt in 1..=max_attempts {
                    match request_fn().await {
                        Ok(val) => return Ok(val),
                        Err(e) => {
                            let is_transport = matches!(&e, LlmError::Http(err) if err.is_timeout() || err.is_connect());
                            if !is_transport {
                                return Err(Box::new(e) as Box<dyn std::error::Error + Send + Sync>);
                            }
                            if attempt == max_attempts {
                                return Err(Box::new(e) as Box<dyn std::error::Error + Send + Sync>);
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

                Err(Box::new(last_error.unwrap_or_else(|| LlmError::NotReachable("max retries exceeded".into()))) as Box<dyn std::error::Error + Send + Sync>)
            })
            .await;

        match result {
            Ok(val) => Ok(val),
            Err(e) => match e {
                soullink_circuit::CircuitBreakerError::Open { .. } => {
                    Err(LlmError::NotReachable("Circuit breaker is OPEN - service unavailable".into()))
                }
                soullink_circuit::CircuitBreakerError::Timeout { .. } => {
                    Err(LlmError::Timeout(self.config.timeout_secs))
                }
                soullink_circuit::CircuitBreakerError::Call { source, .. } => {
                    // Try to downcast to LlmError
                    if let Some(llm_err) = source.downcast_ref::<LlmError>() {
                        Err(LlmError::NotReachable(llm_err.to_string()))
                    } else {
                        Err(LlmError::NotReachable(source.to_string()))
                    }
                }
            },
        }
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

    pub fn fallback_models(&self) -> &[String] {
        &self.config.fallback_models
    }

    async fn chat_with_model(&self, messages: &[ChatMessage], tools: Option<&[ToolSchema]>, model: &str) -> Result<ChatResponse, LlmError> {
        let req = ChatRequest {
            model: model.to_string(),
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

    pub async fn chat_with_fallback(&self, messages: &[ChatMessage], tools: Option<&[ToolSchema]>) -> Result<ChatResponse, LlmError> {
        match self.chat_with_model(messages, tools, &self.config.model).await {
            Ok(resp) => Ok(resp),
Err(e) => {
                    tracing::warn!("Primary model '{}' failed: {}", self.config.model, e);
                    for fallback in &self.config.fallback_models {
                        tracing::info!("Trying fallback model: {}", fallback);
                        match self.chat_with_model(messages, tools, fallback).await {
                            Ok(resp) => return Ok(resp),
                            Err(e2) => tracing::warn!("Fallback model '{}' failed: {}", fallback, e2),
                        }
                    }
                    Err(e)
                }
        }
    }

    async fn generate_with_model(&self, prompt: &str, model: &str) -> Result<GenerateResponse, LlmError> {
        let req = GenerateRequest {
            model: model.to_string(),
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

    pub async fn generate_with_fallback(&self, prompt: &str) -> Result<GenerateResponse, LlmError> {
        match self.generate_with_model(prompt, &self.config.model).await {
            Ok(resp) => Ok(resp),
Err(e) => {
                    tracing::warn!("Primary model '{}' failed: {}", self.config.model, e);
                    for fallback in &self.config.fallback_models {
                        tracing::info!("Trying fallback model: {}", fallback);
                        match self.generate_with_model(prompt, fallback).await {
                            Ok(resp) => return Ok(resp),
                            Err(e2) => tracing::warn!("Fallback model '{}' failed: {}", fallback, e2),
                        }
                    }
                    Err(e)
                }
        }
    }
}

// ── Chat Session (conversation context manager) ───────────────────────

pub struct ChatSession {
    pub messages: Vec<ChatMessage>,
    pub system_prompt: String,
    pub max_context_chars: usize,
}

impl ChatSession {
    pub fn new(system_prompt: &str) -> Self {
        Self {
            messages: Vec::new(),
            system_prompt: system_prompt.to_string(),
            max_context_chars: 32000,
        }
    }

    pub fn with_max_context(system_prompt: &str, max_chars: usize) -> Self {
        Self {
            messages: Vec::new(),
            system_prompt: system_prompt.to_string(),
            max_context_chars: max_chars,
        }
    }

    pub fn add_user_message(&mut self, content: &str) {
        self.messages.push(ChatMessage {
            role: Role::User,
            content: content.to_string(),
            tool_calls: None,
            tool_call_id: None,
        });
    }

    pub fn add_assistant_message(&mut self, content: &str) {
        self.messages.push(ChatMessage {
            role: Role::Assistant,
            content: content.to_string(),
            tool_calls: None,
            tool_call_id: None,
        });
    }

    pub fn add_tool_result(&mut self, tool_call_id: &str, content: &str) {
        self.messages.push(ChatMessage {
            role: Role::Tool,
            content: content.to_string(),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.to_string()),
        });
    }

    pub fn add_assistant_with_tools(&mut self, content: Option<&str>, tool_calls: Vec<ToolCall>) {
        self.messages.push(ChatMessage {
            role: Role::Assistant,
            content: content.unwrap_or("").to_string(),
            tool_calls: Some(tool_calls),
            tool_call_id: None,
        });
    }

    /// Build the full message list with system prompt, truncating old messages if needed
    pub fn build_messages(&self) -> Vec<ChatMessage> {
        let mut messages = vec![ChatMessage {
            role: Role::System,
            content: self.system_prompt.clone(),
            tool_calls: None,
            tool_call_id: None,
        }];

        // Estimate total size, truncate from the front if too large
        let mut total_chars: usize = self.system_prompt.len();
        let mut start_idx = 0;

        for (i, msg) in self.messages.iter().enumerate() {
            let msg_chars = msg.content.len() + 50; // overhead for role/tool_calls
            if total_chars + msg_chars > self.max_context_chars {
                start_idx = i;
                break;
            }
            total_chars += msg_chars;
        }

        if start_idx > 0 {
            // Add a truncation marker
            messages.push(ChatMessage {
                role: Role::User,
                content: format!(
                    "[Context truncated: {} older messages removed]",
                    start_idx
                ),
                tool_calls: None,
                tool_call_id: None,
            });
        }

        messages.extend(self.messages[start_idx..].iter().cloned());
        messages
    }

    pub fn clear(&mut self) {
        self.messages.clear();
    }

    pub fn history_summary(&self) -> String {
        if self.messages.is_empty() {
            return "No conversation history".to_string();
        }
        format!(
            "{} messages, ~{} chars",
            self.messages.len(),
            self.messages.iter().map(|m| m.content.len()).sum::<usize>()
        )
    }
}

// ── Tool Schema Builder ───────────────────────────────────────────────

pub fn build_tool_schemas() -> Vec<ToolSchema> {
    vec![
        ToolSchema {
            schema_type: "function".to_string(),
            function: FunctionSchema {
                name: "execute_shell".to_string(),
                description: "Execute a shell command and return stdout/stderr".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "Shell command to execute"
                        }
                    },
                    "required": ["command"]
                }),
            },
        },
        ToolSchema {
            schema_type: "function".to_string(),
            function: FunctionSchema {
                name: "read_file".to_string(),
                description: "Read file contents with optional line range".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "File path to read"
                        },
                        "start_line": {
                            "type": "integer",
                            "description": "Start line (1-based, optional)"
                        },
                        "num_lines": {
                            "type": "integer",
                            "description": "Number of lines to read (default 200)"
                        }
                    },
                    "required": ["path"]
                }),
            },
        },
        ToolSchema {
            schema_type: "function".to_string(),
            function: FunctionSchema {
                name: "write_file".to_string(),
                description: "Write content to a file (creates or overwrites)".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "File path to write"
                        },
                        "content": {
                            "type": "string",
                            "description": "Content to write"
                        },
                        "mode": {
                            "type": "string",
                            "enum": ["overwrite", "append"],
                            "description": "Write mode (default overwrite)"
                        }
                    },
                    "required": ["path", "content"]
                }),
            },
        },
        ToolSchema {
            schema_type: "function".to_string(),
            function: FunctionSchema {
                name: "patch_file".to_string(),
                description: "Find and replace unique text in a file".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "File path"
                        },
                        "old_text": {
                            "type": "string",
                            "description": "Text to find (must be unique in file)"
                        },
                        "new_text": {
                            "type": "string",
                            "description": "Replacement text"
                        }
                    },
                    "required": ["path", "old_text", "new_text"]
                }),
            },
        },
        ToolSchema {
            schema_type: "function".to_string(),
            function: FunctionSchema {
                name: "list_directory".to_string(),
                description: "List directory contents".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Directory path (default: current)"
                        }
                    }
                }),
            },
        },
        ToolSchema {
            schema_type: "function".to_string(),
            function: FunctionSchema {
                name: "search_files".to_string(),
                description: "Search for files matching a pattern".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "pattern": {
                            "type": "string",
                            "description": "Glob pattern (e.g. '*.rs')"
                        },
                        "path": {
                            "type": "string",
                            "description": "Directory to search in"
                        }
                    },
                    "required": ["pattern"]
                }),
            },
        },
        ToolSchema {
            schema_type: "function".to_string(),
            function: FunctionSchema {
                name: "grep_content".to_string(),
                description: "Search file contents with regex pattern".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "pattern": {
                            "type": "string",
                            "description": "Regex pattern to search for"
                        },
                        "path": {
                            "type": "string",
                            "description": "File or directory to search in"
                        },
                        "file_pattern": {
                            "type": "string",
                            "description": "File filter glob (e.g. '*.rs')"
                        }
                    },
                    "required": ["pattern"]
                }),
            },
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_serde_system() {
        let role = Role::System;
        let json = serde_json::to_string(&role).unwrap();
        assert_eq!(json, "\"system\"");
        let deserialized: Role = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, Role::System);
    }

    #[test]
    fn test_role_serde_user() {
        let role = Role::User;
        let json = serde_json::to_string(&role).unwrap();
        assert_eq!(json, "\"user\"");
        let deserialized: Role = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, Role::User);
    }

    #[test]
    fn test_role_serde_assistant() {
        let role = Role::Assistant;
        let json = serde_json::to_string(&role).unwrap();
        assert_eq!(json, "\"assistant\"");
        let deserialized: Role = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, Role::Assistant);
    }

    #[test]
    fn test_role_serde_tool() {
        let role = Role::Tool;
        let json = serde_json::to_string(&role).unwrap();
        assert_eq!(json, "\"tool\"");
        let deserialized: Role = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, Role::Tool);
    }

    #[test]
    fn test_chat_message_serde() {
        let msg = ChatMessage {
            role: Role::User,
            content: "Hello".to_string(),
            tool_calls: None,
            tool_call_id: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: ChatMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.role, Role::User);
        assert_eq!(deserialized.content, "Hello");
    }

    #[test]
    fn test_tool_call_serde() {
        let tc = ToolCall {
            id: "call_123".to_string(),
            call_type: "function".to_string(),
            function: FunctionCall {
                name: "execute_shell".to_string(),
                arguments: r#"{"command":"ls"}"#.to_string(),
            },
        };
        let json = serde_json::to_string(&tc).unwrap();
        let deserialized: ToolCall = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, "call_123");
        assert_eq!(deserialized.function.name, "execute_shell");
    }

    #[test]
    fn test_tool_schema_serde() {
        let schema = ToolSchema {
            schema_type: "function".to_string(),
            function: FunctionSchema {
                name: "test_tool".to_string(),
                description: "A test tool".to_string(),
                parameters: serde_json::json!({"type": "object", "properties": {}}),
            },
        };
        let json = serde_json::to_string(&schema).unwrap();
        let deserialized: ToolSchema = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.function.name, "test_tool");
    }

    #[test]
    fn test_llm_config_default() {
        // Default config without env vars should use fallbacks
        let config = LlmConfig::default();
        assert_eq!(config.base_url, "http://127.0.0.1:11434");
        assert_eq!(config.temperature, 0.7);
        assert_eq!(config.max_tokens, 4096);
    }

    #[test]
    fn test_llm_config_builder() {
        let config = LlmConfig::default()
            .with_model("test-model")
            .with_temperature(0.5)
            .with_max_tokens(1024);
        assert_eq!(config.model, "test-model");
        assert_eq!(config.temperature, 0.5);
        assert_eq!(config.max_tokens, 1024);
    }

    #[test]
    fn test_llm_config_for_complexity() {
        let low = LlmConfig::for_complexity(1);
        assert_eq!(low.max_tokens, 2048);

        let med = LlmConfig::for_complexity(5);
        assert_eq!(med.max_tokens, 4096);

        let high = LlmConfig::for_complexity(7);
        assert_eq!(high.max_tokens, 8192);
    }

    #[test]
    fn test_llm_config_fallback_models() {
        let config = LlmConfig::default();
        assert!(!config.fallback_models.is_empty());
    }

    #[test]
    fn test_chat_session_add_user() {
        let mut session = ChatSession::new("test system prompt");
        session.add_user_message("hello");
        assert_eq!(session.messages.len(), 1);
        assert_eq!(session.messages[0].role, Role::User);
    }

    #[test]
    fn test_chat_session_add_assistant() {
        let mut session = ChatSession::new("test system prompt");
        session.add_assistant_message("response");
        assert_eq!(session.messages.len(), 1);
        assert_eq!(session.messages[0].role, Role::Assistant);
    }

    #[test]
    fn test_chat_session_add_tool_result() {
        let mut session = ChatSession::new("test system prompt");
        session.add_tool_result("tool_1", "output");
        assert_eq!(session.messages.len(), 1);
        assert_eq!(session.messages[0].role, Role::Tool);
        assert_eq!(session.messages[0].tool_call_id, Some("tool_1".to_string()));
    }

    #[test]
    fn test_chat_session_build_messages() {
        let mut session = ChatSession::new("system prompt");
        session.add_user_message("user message");
        let messages = session.build_messages();
        assert_eq!(messages.len(), 2); // system + user
        assert_eq!(messages[0].role, Role::System);
        assert_eq!(messages[0].content, "system prompt");
        assert_eq!(messages[1].role, Role::User);
    }

    #[test]
    fn test_chat_session_context_truncation() {
        // Create a session with very small context limit that fits system + 1 small msg
        let mut session = ChatSession::with_max_context("ab", 132); // system=2 chars, +50 overhead = fits ~80 chars of msg
        // First message: fits within limit
        session.add_user_message("hello world"); // 11 chars + 50 overhead = 61, system 2+50 = 52, total 113 <= 132
        // Second message: pushes over the limit
        session.add_user_message("this second message is longer and will push us over the limit definitely"); // 86+50=136, total 113+136=249 > 132
        let messages = session.build_messages();
        // Should include system + truncation marker + second message
        assert!(
            messages.len() == 3,
            "expected 3 messages (system + truncation + 1 kept), got {}",
            messages.len()
        );
        assert_eq!(messages[0].role, Role::System);
        assert!(
            messages[1].content.contains("truncat"),
            "expected truncation marker, got: {:?}",
            messages[1].content
        );
        assert_eq!(messages[2].content, "this second message is longer and will push us over the limit definitely");
    }

    #[test]
    fn test_chat_session_clear() {
        let mut session = ChatSession::new("system");
        session.add_user_message("msg");
        assert_eq!(session.messages.len(), 1);
        session.clear();
        assert_eq!(session.messages.len(), 0);
    }

    #[test]
    fn test_chat_session_history_summary() {
        let session = ChatSession::new("system");
        assert_eq!(session.history_summary(), "No conversation history");
    }

    #[test]
    fn test_chat_session_with_max_context() {
        let session = ChatSession::with_max_context("system", 500);
        assert_eq!(session.max_context_chars, 500);
    }

    #[test]
    fn test_build_tool_schemas() {
        let schemas = build_tool_schemas();
        assert!(!schemas.is_empty());
        assert!(schemas.iter().any(|s| s.function.name == "execute_shell"));
        assert!(schemas.iter().any(|s| s.function.name == "read_file"));
        assert!(schemas.iter().any(|s| s.function.name == "write_file"));
    }

    #[test]
    fn test_chat_response_message_deser() {
        let json = r#"{"message":{"role":"assistant","content":"Hello","tool_calls":null},"done":true,"total_duration":1000,"eval_count":10}"#;
        let resp: ChatResponse = serde_json::from_str(json).unwrap();
        assert!(resp.done);
        assert_eq!(resp.message.content.unwrap(), "Hello");
        assert_eq!(resp.total_duration, Some(1000));
    }

    #[test]
    fn test_chat_response_with_tool_calls_deser() {
        let json = r#"{"message":{"role":"assistant","content":null,"tool_calls":[{"id":"call_1","type":"function","function":{"name":"execute_shell","arguments":"{\"command\":\"ls\"}"}}]},"done":true}"#;
        let resp: ChatResponse = serde_json::from_str(json).unwrap();
        assert!(resp.done);
        let calls = resp.message.tool_calls.unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "execute_shell");
    }

    #[test]
    fn test_generate_response_deser() {
        let json = r#"{"response":"Hello!","done":true,"total_duration":500,"eval_count":5}"#;
        let resp: GenerateResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.response, "Hello!");
        assert!(resp.done);
    }

    #[test]
    fn test_embed_response_deser() {
        let json = r#"{"embeddings":[[0.1,0.2,0.3]]}"#;
        let resp: EmbedResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.embeddings.len(), 1);
        assert_eq!(resp.embeddings[0], vec![0.1, 0.2, 0.3]);
    }

    #[test]
    fn test_stream_chunk_deser() {
        let json = r#"{"message":{"role":"assistant","content":"Hello"},"done":false}"#;
        let chunk: StreamChunk = serde_json::from_str(json).unwrap();
        assert!(!chunk.done);
        assert_eq!(chunk.message.unwrap().content.unwrap(), "Hello");
    }

    #[test]
    fn test_stream_chunk_done() {
        let json = r#"{"message":{"role":"assistant","content":"World"},"done":true}"#;
        let chunk: StreamChunk = serde_json::from_str(json).unwrap();
        assert!(chunk.done);
    }

    #[test]
    fn test_models_response_deser() {
        let json = r#"{"models":[{"name":"llama3:8b","size":1000,"parameter_size":"8B"}]}"#;
        let resp: ModelsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.models.len(), 1);
        assert_eq!(resp.models[0].name, "llama3:8b");
    }

    #[test]
    fn test_llm_error_display() {
        let err = LlmError::NotReachable("test".to_string());
        assert_eq!(format!("{}", err), "Ollama not reachable at test");
    }

    #[test]
    fn test_function_schema_serde() {
        let schema = FunctionSchema {
            name: "test".to_string(),
            description: "desc".to_string(),
            parameters: serde_json::json!({"type": "object"}),
        };
        let json = serde_json::to_string(&schema).unwrap();
        let deser: FunctionSchema = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.name, "test");
    }

    // ── Additional: Role edge cases ──

    #[test]
    fn test_role_deser_invalid_fails() {
        let result: Result<Role, _> = serde_json::from_str("\"superuser\"");
        assert!(result.is_err(), "unknown role string should fail to deserialize");
    }

    #[test]
    fn test_role_deser_case_sensitive() {
        let result: Result<Role, _> = serde_json::from_str("\"User\"");
        assert!(result.is_err(), "capitalized User should fail (lowercase expected)");
    }

    #[test]
    fn test_role_deser_via_chat_response() {
        let json = r#"{"message":{"role":"tool","content":"result"},"done":true}"#;
        let resp: ChatResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.message.role, "tool");
        assert_eq!(resp.message.content.as_deref(), Some("result"));
    }

    // ── Additional: ChatMessage edge cases ──

    #[test]
    fn test_chat_message_with_tool_call_id() {
        let msg = ChatMessage {
            role: Role::Tool,
            content: "file content".to_string(),
            tool_calls: None,
            tool_call_id: Some("call_file_1".to_string()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("tool_call_id"), "tool_call_id should be present in JSON: {}", json);
        let deser: ChatMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.tool_call_id.as_deref(), Some("call_file_1"));
        assert_eq!(deser.role, Role::Tool);
    }

    #[test]
    fn test_chat_message_with_tool_calls() {
        let tc = ToolCall {
            id: "ct_1".to_string(),
            call_type: "function".to_string(),
            function: FunctionCall {
                name: "read_file".to_string(),
                arguments: r#"{"path":"/tmp/x"}"#.to_string(),
            },
        };
        let msg = ChatMessage {
            role: Role::Assistant,
            content: String::new(),
            tool_calls: Some(vec![tc]),
            tool_call_id: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("tool_calls"), "tool_calls should be present: {}", json);
        let deser: ChatMessage = serde_json::from_str(&json).unwrap();
        assert!(deser.tool_calls.is_some());
        assert_eq!(deser.tool_calls.as_ref().unwrap().len(), 1);
        assert_eq!(deser.tool_calls.as_ref().unwrap()[0].function.name, "read_file");
    }

    #[test]
    fn test_chat_message_with_both_tool_fields() {
        let tc = ToolCall {
            id: "ct_2".to_string(),
            call_type: "function".to_string(),
            function: FunctionCall {
                name: "search".to_string(),
                arguments: "{}".to_string(),
            },
        };
        let msg = ChatMessage {
            role: Role::Assistant,
            content: "using tools".to_string(),
            tool_calls: Some(vec![tc]),
            tool_call_id: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let deser: ChatMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.content, "using tools");
        assert!(deser.tool_calls.is_some());
        assert_eq!(deser.tool_call_id, None);
    }

    #[test]
    fn test_chat_message_skips_optional_none() {
        let msg = ChatMessage {
            role: Role::User,
            content: "skip test".to_string(),
            tool_calls: None,
            tool_call_id: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(!json.contains("tool_calls"), "should skip tool_calls: {}", json);
        assert!(!json.contains("tool_call_id"), "should skip tool_call_id: {}", json);
    }

    #[test]
    fn test_chat_message_empty_content() {
        let msg = ChatMessage {
            role: Role::Assistant,
            content: String::new(),
            tool_calls: None,
            tool_call_id: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let deser: ChatMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.content, "");
    }

    // ── Additional: ToolCall / FunctionCall edge cases ──

    #[test]
    fn test_tool_call_with_special_chars_in_id() {
        let tc = ToolCall {
            id: "call_abc-123_xyz".to_string(),
            call_type: "function".to_string(),
            function: FunctionCall {
                name: "grep_content".to_string(),
                arguments: r#"{"pattern":"fn main","path":"src/"}"#.to_string(),
            },
        };
        let json = serde_json::to_string(&tc).unwrap();
        let deser: ToolCall = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.id, "call_abc-123_xyz");
        assert_eq!(deser.function.name, "grep_content");
    }

    #[test]
    fn test_function_call_empty_arguments() {
        let fc = FunctionCall {
            name: "noop".to_string(),
            arguments: "{}".to_string(),
        };
        let json = serde_json::to_string(&fc).unwrap();
        let deser: FunctionCall = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.arguments, "{}");
    }

    #[test]
    fn test_tool_call_type_alias() {
        let tc = ToolCall {
            id: "t1".to_string(),
            call_type: "function".to_string(),
            function: FunctionCall {
                name: "test".to_string(),
                arguments: "{}".to_string(),
            },
        };
        let json = serde_json::to_string(&tc).unwrap();
        // The field should serialize as "type" (the serde rename)
        assert!(json.contains("\"type\""), "field should serialize as 'type': {}", json);
    }

    // ── Additional: ToolSchema / FunctionSchema edge cases ──

    #[test]
    fn test_tool_schema_type_alias() {
        let schema = ToolSchema {
            schema_type: "function".to_string(),
            function: FunctionSchema {
                name: "any_tool".to_string(),
                description: "desc".to_string(),
                parameters: serde_json::json!({"type": "object"}),
            },
        };
        let json = serde_json::to_string(&schema).unwrap();
        // Should serialize schema_type as "type"
        assert!(json.contains("\"type\""), "should serialize as 'type': {}", json);
        let deser: ToolSchema = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.schema_type, "function");
    }

    #[test]
    fn test_function_schema_empty_description() {
        let schema = FunctionSchema {
            name: "silent_tool".to_string(),
            description: String::new(),
            parameters: serde_json::json!({"type": "object", "properties": {}}),
        };
        let json = serde_json::to_string(&schema).unwrap();
        let deser: FunctionSchema = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.description, "");
    }

    #[test]
    fn test_function_schema_complex_parameters() {
        let schema = FunctionSchema {
            name: "complex".to_string(),
            description: "complex params".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string"},
                    "count": {"type": "integer", "minimum": 0},
                    "tags": {"type": "array", "items": {"type": "string"}},
                    "config": {"type": "object", "properties": {"debug": {"type": "boolean"}}}
                },
                "required": ["name", "count"]
            }),
        };
        let json = serde_json::to_string(&schema).unwrap();
        let deser: FunctionSchema = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.name, "complex");
        let params = deser.parameters.as_object().unwrap();
        assert!(params.contains_key("properties"));
        let props = params["properties"].as_object().unwrap();
        assert!(props.contains_key("tags"));
    }

    // ── Private request struct serialization ──

    #[test]
    fn test_generate_request_serialization() {
        let req = GenerateRequest {
            model: "test-model".to_string(),
            prompt: "hello".to_string(),
            stream: false,
            options: Some(GenerateOptions {
                temperature: 0.5,
                num_predict: 512,
            }),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"model\":\"test-model\""));
        assert!(json.contains("\"prompt\":\"hello\""));
        assert!(json.contains("\"stream\":false"));
        assert!(json.contains("\"temperature\":0.5"));
        assert!(json.contains("\"num_predict\":512"));
    }

    #[test]
    fn test_chat_request_serialization_with_tools() {
        let req = ChatRequest {
            model: "qwen3".to_string(),
            messages: vec![ChatMessage {
                role: Role::User,
                content: "hi".to_string(),
                tool_calls: None,
                tool_call_id: None,
            }],
            stream: false,
            tools: Some(vec![ToolSchema {
                schema_type: "function".to_string(),
                function: FunctionSchema {
                    name: "my_tool".to_string(),
                    description: "".to_string(),
                    parameters: serde_json::json!({"type": "object"}),
                },
            }]),
            options: Some(GenerateOptions {
                temperature: 0.7,
                num_predict: 4096,
            }),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"model\":\"qwen3\""));
        assert!(json.contains("\"tools\""));
        assert!(json.contains("\"my_tool\""));
    }

    #[test]
    fn test_chat_request_serialization_without_tools() {
        let req = ChatRequest {
            model: "m".to_string(),
            messages: vec![],
            stream: true,
            tools: None,
            options: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        // tools and options are Option fields without skip_serializing_if, so they serialize as null
        assert!(json.contains("\"tools\":null"), "expected tools:null in: {}", json);
        assert!(json.contains("\"options\":null"), "expected options:null in: {}", json);
        assert!(json.contains("\"stream\":true"));
    }

    #[test]
    fn test_embed_request_serialization() {
        let req = EmbedRequest {
            model: "embed-model".to_string(),
            input: vec!["text one".to_string(), "text two".to_string()],
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"model\":\"embed-model\""));
        assert!(json.contains("\"text one\""));
        assert!(json.contains("\"text two\""));
    }

    #[test]
    fn test_generate_options_serialization() {
        let opts = GenerateOptions {
            temperature: 0.0,
            num_predict: 1,
        };
        let json = serde_json::to_string(&opts).unwrap();
        assert!(json.contains("\"temperature\":0.0"));
        assert!(json.contains("\"num_predict\":1"));
    }

    // ── Additional: ChatSession edge cases ──

    #[test]
    fn test_chat_session_add_assistant_with_tools_some_content() {
        let mut session = ChatSession::new("sys");
        let tc = ToolCall {
            id: "ct_99".to_string(),
            call_type: "function".to_string(),
            function: FunctionCall {
                name: "list_directory".to_string(),
                arguments: "{}".to_string(),
            },
        };
        session.add_assistant_with_tools(Some("I'll list the dir"), vec![tc]);
        assert_eq!(session.messages.len(), 1);
        assert_eq!(session.messages[0].role, Role::Assistant);
        assert_eq!(session.messages[0].content, "I'll list the dir");
        let calls = session.messages[0].tool_calls.as_ref().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "list_directory");
    }

    #[test]
    fn test_chat_session_add_assistant_with_tools_none_content() {
        let mut session = ChatSession::new("sys");
        let tc = ToolCall {
            id: "ct_98".to_string(),
            call_type: "function".to_string(),
            function: FunctionCall {
                name: "write_file".to_string(),
                arguments: "{}".to_string(),
            },
        };
        session.add_assistant_with_tools(None, vec![tc]);
        assert_eq!(session.messages[0].content, "");
        assert!(session.messages[0].tool_calls.is_some());
    }

    #[test]
    fn test_chat_session_add_assistant_with_tools_multiple_calls() {
        let mut session = ChatSession::new("sys");
        let calls = vec![
            ToolCall {
                id: "ct_1".to_string(),
                call_type: "function".to_string(),
                function: FunctionCall {
                    name: "read_file".to_string(),
                    arguments: r#"{"path":"a.txt"}"#.to_string(),
                },
            },
            ToolCall {
                id: "ct_2".to_string(),
                call_type: "function".to_string(),
                function: FunctionCall {
                    name: "write_file".to_string(),
                    arguments: r#"{"path":"b.txt","content":"data"}"#.to_string(),
                },
            },
        ];
        session.add_assistant_with_tools(None, calls);
        let msg = &session.messages[0];
        let tool_calls = msg.tool_calls.as_ref().unwrap();
        assert_eq!(tool_calls.len(), 2);
        assert_eq!(tool_calls[0].id, "ct_1");
        assert_eq!(tool_calls[1].id, "ct_2");
    }

    #[test]
    fn test_chat_session_build_messages_truncation_at_start() {
        // Extremely tight limit: system prompt fits, first user message barely fits,
        // second user message pushes over the limit -> truncation at index 1
        let mut session = ChatSession::with_max_context("sys", 55); // system len 3
        session.add_user_message("hi"); // 2 chars + 50 overhead = 52, total 3+52=55 <=55, fits
        session.add_user_message("this second message exceeds the limit and triggers truncation");
        let messages = session.build_messages();
        assert_eq!(messages.len(), 3, "system + truncation marker + 1 kept = 3, got {}", messages.len());
        assert_eq!(messages[0].role, Role::System);
        assert!(messages[1].content.contains("truncat"));
        assert_eq!(messages[2].role, Role::User);
    }

    #[test]
    fn test_chat_session_build_messages_exact_fit() {
        // system "s" = 1 char, user "12345678901" = 11 chars + 50 overhead = 61, total = 62 == max
        // No truncation since 62 is NOT > 62
        let mut session = ChatSession::with_max_context("s", 62);
        session.add_user_message("12345678901"); // 11 chars
        let messages = session.build_messages();
        assert_eq!(messages.len(), 2, "system + user = 2 messages when total exactly fits max_context");
    }

    #[test]
    fn test_chat_session_build_messages_multiple_tool_results() {
        let mut session = ChatSession::new("sys");
        session.add_tool_result("t1", "out1");
        session.add_tool_result("t2", "out2");
        session.add_user_message("continue");
        let messages = session.build_messages();
        // system + 3 messages = 4
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[0].role, Role::System);
        assert_eq!(messages[1].role, Role::Tool);
        assert_eq!(messages[1].tool_call_id.as_deref(), Some("t1"));
        assert_eq!(messages[2].role, Role::Tool);
        assert_eq!(messages[2].tool_call_id.as_deref(), Some("t2"));
        assert_eq!(messages[3].role, Role::User);
    }

    #[test]
    fn test_chat_session_build_messages_empty_system_prompt() {
        let mut session = ChatSession::new("");
        session.add_user_message("hello");
        let messages = session.build_messages();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].content, "");
    }

    #[test]
    fn test_chat_session_history_summary_with_messages() {
        let mut session = ChatSession::new("sys");
        session.add_user_message("abc");
        session.add_assistant_message("def");
        let summary = session.history_summary();
        assert!(summary.contains("2 messages"), "expected 2 messages in summary: {}", summary);
        assert!(summary.contains("6"), "expected 6 chars (abc=3 + def=3) in summary: {}", summary);
    }

    #[test]
    fn test_chat_session_history_summary_after_clear() {
        let mut session = ChatSession::new("sys");
        session.add_user_message("data");
        session.clear();
        assert_eq!(session.history_summary(), "No conversation history");
    }

    #[test]
    fn test_chat_session_max_context_preserved() {
        let session = ChatSession::with_max_context("system prompt", 9999);
        assert_eq!(session.max_context_chars, 9999);
        assert_eq!(session.system_prompt, "system prompt");
    }

    // ── Additional: LlmConfig edge cases ──

    #[test]
    fn test_llm_config_for_complexity_boundaries() {
        // complexity < 3 (threshold)
        assert_eq!(LlmConfig::for_complexity(0).max_tokens, 2048);
        assert_eq!(LlmConfig::for_complexity(2).max_tokens, 2048);
        // complexity >= 3 and < 6
        assert_eq!(LlmConfig::for_complexity(3).max_tokens, 4096);
        assert_eq!(LlmConfig::for_complexity(5).max_tokens, 4096);
        // complexity >= 6
        assert_eq!(LlmConfig::for_complexity(6).max_tokens, 8192);
        assert_eq!(LlmConfig::for_complexity(255).max_tokens, 8192);
    }

    #[test]
    fn test_llm_config_for_complexity_models() {
        let low = LlmConfig::for_complexity(1);
        assert_eq!(low.model, "qwen3:4b");
        let med = LlmConfig::for_complexity(5);
        assert_eq!(med.model, "qwen3:8b");
        let high = LlmConfig::for_complexity(9);
        assert_eq!(high.model, "qwen3:8b");
    }

    #[test]
    fn test_llm_config_with_fallback_models() {
        let config = LlmConfig::default()
            .with_fallback_models(vec!["m1".to_string(), "m2".to_string()]);
        assert_eq!(config.fallback_models.len(), 2);
        assert_eq!(config.fallback_models[0], "m1");
    }

    #[test]
    fn test_llm_config_builder_preserves_other_fields() {
        let config = LlmConfig::default()
            .with_model("custom")
            .with_temperature(0.1)
            .with_max_tokens(100);
        // Check builder did not corrupt other fields
        assert_eq!(config.base_url, "http://127.0.0.1:11434");
        assert!(config.fallback_models.len() >= 1);
        assert_eq!(config.timeout_secs, 120);
        assert_eq!(config.max_retries, 3);
    }

    #[test]
    fn test_llm_config_serde_roundtrip() {
        let config = LlmConfig {
            base_url: "http://localhost:8080".to_string(),
            model: "custom:latest".to_string(),
            fallback_models: vec!["a".to_string(), "b".to_string()],
            temperature: 0.3,
            max_tokens: 2048,
            system_prompt: Some("You are a helpful assistant.".to_string()),
            timeout_secs: 60,
            max_retries: 5,
            retry_base_delay_ms: 500,
        };
        let json = serde_json::to_string(&config).unwrap();
        let deser: LlmConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.base_url, "http://localhost:8080");
        assert_eq!(deser.model, "custom:latest");
        assert_eq!(deser.fallback_models.len(), 2);
        assert_eq!(deser.temperature, 0.3);
        assert_eq!(deser.max_tokens, 2048);
        assert_eq!(deser.system_prompt.as_deref(), Some("You are a helpful assistant."));
        assert_eq!(deser.timeout_secs, 60);
        assert_eq!(deser.max_retries, 5);
        assert_eq!(deser.retry_base_delay_ms, 500);
    }

    #[test]
    fn test_llm_config_system_prompt_none() {
        let config = LlmConfig {
            base_url: "http://x:11434".to_string(),
            model: "m".to_string(),
            fallback_models: vec![],
            temperature: 0.5,
            max_tokens: 100,
            system_prompt: None,
            timeout_secs: 30,
            max_retries: 2,
            retry_base_delay_ms: 200,
        };
        let json = serde_json::to_string(&config).unwrap();
        // system_prompt is Option but has no skip_serializing_if, so null is present
        assert!(json.contains("\"system_prompt\":null"), "expected system_prompt:null in: {}", json);
    }

    // ── ChatResponseMessage edge cases ──

    #[test]
    fn test_chat_response_message_null_content() {
        let json = r#"{"message":{"role":"assistant","content":null},"done":true}"#;
        let resp: ChatResponse = serde_json::from_str(json).unwrap();
        assert!(resp.message.content.is_none());
    }

    #[test]
    fn test_chat_response_message_with_tool_calls() {
        let json = r#"{"message":{"role":"assistant","content":null,"tool_calls":[{"id":"tc_1","type":"function","function":{"name":"search_files","arguments":"{\"pattern\":\"*.rs\"}"}}]},"done":true,"total_duration":200}"#;
        let resp: ChatResponse = serde_json::from_str(json).unwrap();
        let calls = resp.message.tool_calls.unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "search_files");
        assert_eq!(calls[0].function.arguments, r#"{"pattern":"*.rs"}"#);
        assert_eq!(resp.total_duration, Some(200));
    }

    // ── StreamChunk edge cases ──

    #[test]
    fn test_stream_chunk_with_null_content() {
        let json = r#"{"message":{"role":"assistant","content":null,"tool_calls":[{"id":"st_1","type":"function","function":{"name":"execute_shell","arguments":"{}"}}]},"done":false}"#;
        let chunk: StreamChunk = serde_json::from_str(json).unwrap();
        assert!(!chunk.done);
        let msg = chunk.message.unwrap();
        assert!(msg.content.is_none());
        let calls = msg.tool_calls.unwrap();
        assert_eq!(calls.len(), 1);
    }

    #[test]
    fn test_stream_chunk_without_message_field() {
        // Some Ollama lines may only have {"done":true} at the end
        let json = r#"{"done":true}"#;
        let chunk: StreamChunk = serde_json::from_str(json).unwrap();
        assert!(chunk.done);
        assert!(chunk.message.is_none());
    }

    #[test]
    fn test_stream_chunk_empty_content() {
        let json = r#"{"message":{"role":"assistant","content":"","tool_calls":null},"done":false}"#;
        let chunk: StreamChunk = serde_json::from_str(json).unwrap();
        assert_eq!(chunk.message.as_ref().unwrap().content.as_deref(), Some(""));
    }

    #[test]
    fn test_stream_chunk_partial_message() {
        // StreamChunk with role and content but no tool_calls
        let json = r#"{"message":{"role":"assistant","content":"Hel"},"done":false}"#;
        let chunk: StreamChunk = serde_json::from_str(json).unwrap();
        assert_eq!(chunk.message.unwrap().role.as_deref(), Some("assistant"));
    }

    // ── EmbedResponse edge cases ──

    #[test]
    fn test_embed_response_empty() {
        let json = r#"{"embeddings":[]}"#;
        let resp: EmbedResponse = serde_json::from_str(json).unwrap();
        assert!(resp.embeddings.is_empty());
    }

    #[test]
    fn test_embed_response_multiple() {
        let json = r#"{"embeddings":[[0.5,-0.5],[1.0,2.0,3.0]]}"#;
        let resp: EmbedResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.embeddings.len(), 2);
        assert_eq!(resp.embeddings[0].len(), 2);
        assert_eq!(resp.embeddings[1].len(), 3);
    }

    // ── ModelsResponse edge cases ──

    #[test]
    fn test_models_response_empty() {
        let json = r#"{"models":[]}"#;
        let resp: ModelsResponse = serde_json::from_str(json).unwrap();
        assert!(resp.models.is_empty());
    }

    #[test]
    fn test_models_response_null_fields() {
        let json = r#"{"models":[{"name":"qwen3:8b","size":null,"parameter_size":null}]}"#;
        let resp: ModelsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.models[0].name, "qwen3:8b");
        assert!(resp.models[0].size.is_none());
        assert!(resp.models[0].parameter_size.is_none());
    }

    // ── LlmError all variants ──

    #[test]
    fn test_llm_error_http_display() {
        let err = LlmError::Timeout(30);
        assert_eq!(format!("{}", err), "Timeout after 30s");
    }

    #[test]
    fn test_llm_error_stream_display() {
        let err = LlmError::Stream("connection lost".to_string());
        assert_eq!(format!("{}", err), "Streaming error: connection lost");
    }

    #[test]
    fn test_llm_error_json_from_serde() {
        let json_err = serde_json::from_str::<Role>("invalid").unwrap_err();
        let err = LlmError::Json(json_err);
        let display = format!("{}", err);
        assert!(display.starts_with("JSON error:"), "expected JSON error prefix, got: {}", display);
    }

    #[test]
    fn test_llm_error_http_via_from() {
        // Verify the From<reqwest::Error> impl compiles and works
        // We can't easily create a real reqwest::Error, but we can verify the trait bound
        fn assert_from_trait<T: From<reqwest::Error>>() {}
        assert_from_trait::<LlmError>();
    }

    // ── Api struct deserialization variations ──

    #[test]
    fn test_generate_response_no_duration() {
        let json = r#"{"response":"test","done":true}"#;
        let resp: GenerateResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.response, "test");
        assert!(resp.done);
        assert!(resp.total_duration.is_none());
        assert!(resp.eval_count.is_none());
    }

    #[test]
    fn test_generate_response_not_done() {
        let json = r#"{"response":"partial","done":false}"#;
        let resp: GenerateResponse = serde_json::from_str(json).unwrap();
        assert!(!resp.done);
    }

    // ── build_messages edge: empty messages list ──

    #[test]
    fn test_chat_session_build_messages_no_history() {
        let session = ChatSession::new("sys only");
        let messages = session.build_messages();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, Role::System);
        assert_eq!(messages[0].content, "sys only");
    }

    // ── build_tool_schemas ──

    #[test]
    fn test_build_tool_schemas_all_present() {
        let schemas = build_tool_schemas();
        let names: Vec<&str> = schemas.iter().map(|s| s.function.name.as_str()).collect();
        for expected in &["execute_shell", "read_file", "write_file", "patch_file", "list_directory", "search_files", "grep_content"] {
            assert!(names.contains(expected), "missing tool: {}", expected);
        }
    }

    #[test]
    fn test_build_tool_schemas_all_have_descriptions() {
        let schemas = build_tool_schemas();
        for schema in &schemas {
            assert!(!schema.function.description.is_empty(), "tool '{}' has empty description", schema.function.name);
            assert_eq!(schema.schema_type, "function");
        }
    }

    #[test]
    fn test_build_tool_schemas_all_have_parameters() {
        let schemas = build_tool_schemas();
        for schema in &schemas {
            let params = &schema.function.parameters;
            assert!(params.is_object(), "tool '{}' parameters should be an object", schema.function.name);
        }
    }

    // ── ChatRequest tool serialization ──

    #[test]
    fn test_chat_request_with_tools_has_correct_structure() {
        let tools = build_tool_schemas();
        let msg = ChatMessage {
            role: Role::User,
            content: "list files".to_string(),
            tool_calls: None,
            tool_call_id: None,
        };
        let req = ChatRequest {
            model: "m".to_string(),
            messages: vec![msg],
            stream: false,
            tools: Some(tools),
            options: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let tools_arr = parsed["tools"].as_array().unwrap();
        assert!(tools_arr.len() >= 7);
        assert_eq!(tools_arr[0]["function"]["name"], "execute_shell");
    }
}
