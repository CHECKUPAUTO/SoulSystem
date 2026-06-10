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
            Ok(resp) => return Ok(resp),
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
            Ok(resp) => return Ok(resp),
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
