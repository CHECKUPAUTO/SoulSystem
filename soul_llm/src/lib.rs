use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;

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

fn default_max_retries() -> u32 {
    3
}

fn default_retry_base_delay_ms() -> u64 {
    1000
}

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

// ── Chat Session (conversation context manager) ────────────────────────

/// Maintains an ordered conversation around a fixed system prompt, supporting
/// tool calls and tool results. Autonomous agents use it to keep context
/// coherent across multiple reason→act turns.
#[derive(Debug, Clone)]
pub struct ChatSession {
    pub messages: Vec<ChatMessage>,
    pub max_context_chars: usize,
    system_prompt: String,
}

impl ChatSession {
    /// Default character budget before compaction becomes advisable.
    pub const DEFAULT_MAX_CONTEXT_CHARS: usize = 40_000;

    pub fn new(system_prompt: &str) -> Self {
        Self::with_max_context(system_prompt, Self::DEFAULT_MAX_CONTEXT_CHARS)
    }

    pub fn with_max_context(system_prompt: &str, max_context_chars: usize) -> Self {
        let mut session = Self {
            messages: Vec::new(),
            max_context_chars,
            system_prompt: system_prompt.to_string(),
        };
        if !system_prompt.is_empty() {
            session.messages.push(ChatMessage {
                role: Role::System,
                content: system_prompt.to_string(),
                tool_calls: None,
                tool_call_id: None,
            });
        }
        session
    }

    /// Reset the conversation, keeping only the system prompt.
    pub fn clear(&mut self) {
        self.messages.retain(|m| m.role == Role::System);
        if self.messages.is_empty() && !self.system_prompt.is_empty() {
            self.messages.push(ChatMessage {
                role: Role::System,
                content: self.system_prompt.clone(),
                tool_calls: None,
                tool_call_id: None,
            });
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

    pub fn add_assistant_with_tools(&mut self, content: Option<&str>, tool_calls: Vec<ToolCall>) {
        self.messages.push(ChatMessage {
            role: Role::Assistant,
            content: content.unwrap_or("").to_string(),
            tool_calls: if tool_calls.is_empty() {
                None
            } else {
                Some(tool_calls)
            },
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

    /// Snapshot of the conversation to send to the LLM.
    pub fn build_messages(&self) -> Vec<ChatMessage> {
        self.messages.clone()
    }

    /// Total characters currently held across all messages.
    pub fn total_chars(&self) -> usize {
        self.messages.iter().map(|m| m.content.len()).sum()
    }

    /// Compact, human-readable transcript of the conversation so far.
    pub fn history_summary(&self) -> String {
        self.messages
            .iter()
            .map(|m| {
                let role = match m.role {
                    Role::System => "system",
                    Role::User => "user",
                    Role::Assistant => "assistant",
                    Role::Tool => "tool",
                };
                let content: String = m.content.chars().take(200).collect();
                format!("{}: {}", role, content)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Default tool schemas advertised to the LLM. These mirror the tools handled
/// by `soul_tools::dispatch_tool`, so the model only requests calls the agent
/// can actually execute.
pub fn build_tool_schemas() -> Vec<ToolSchema> {
    use serde_json::json;

    let tool = |name: &str, description: &str, parameters: serde_json::Value| ToolSchema {
        schema_type: "function".to_string(),
        function: FunctionSchema {
            name: name.to_string(),
            description: description.to_string(),
            parameters,
        },
    };

    vec![
        tool(
            "execute_shell",
            "Execute a shell command and return its stdout/stderr.",
            json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "The shell command to run" }
                },
                "required": ["command"]
            }),
        ),
        tool(
            "read_file",
            "Read a text file, optionally limited to a line range.",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Path to the file" },
                    "start_line": { "type": "integer", "description": "First line, 1-based (optional)" },
                    "num_lines": { "type": "integer", "description": "Number of lines to read (optional)" }
                },
                "required": ["path"]
            }),
        ),
        tool(
            "write_file",
            "Write or append text content to a file.",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Path to the file" },
                    "content": { "type": "string", "description": "Content to write" },
                    "mode": { "type": "string", "enum": ["overwrite", "append"], "description": "Write mode (default overwrite)" }
                },
                "required": ["path", "content"]
            }),
        ),
        tool(
            "patch_file",
            "Replace an exact text fragment within a file.",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Path to the file" },
                    "old_text": { "type": "string", "description": "Exact text to replace" },
                    "new_text": { "type": "string", "description": "Replacement text" }
                },
                "required": ["path", "old_text", "new_text"]
            }),
        ),
        tool(
            "list_directory",
            "List the entries of a directory.",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Directory path (default '.')" }
                }
            }),
        ),
        tool(
            "search_files",
            "Find files matching a name pattern under a path.",
            json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "description": "Filename pattern" },
                    "path": { "type": "string", "description": "Root path (default '.')" }
                },
                "required": ["pattern"]
            }),
        ),
        tool(
            "grep_content",
            "Search file contents for a regular expression.",
            json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "description": "Regex to search for" },
                    "path": { "type": "string", "description": "Root path (default '.')" },
                    "file_pattern": { "type": "string", "description": "Optional filename filter" }
                },
                "required": ["pattern"]
            }),
        ),
    ]
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
                    let is_transport =
                        matches!(&e, LlmError::Http(err) if err.is_timeout() || err.is_connect());
                    if !is_transport {
                        return Err(e);
                    }
                    if attempt == max_attempts {
                        return Err(e);
                    }
                    last_error = Some(e);
                    let delay_ms = self
                        .config
                        .retry_base_delay_ms
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

// Blocking versions for REPL and non-async contexts.
//
// Each request runs on a dedicated OS thread: reqwest::blocking panics when
// created or dropped inside a tokio runtime, and these methods are called
// from both pure-sync code and `block_on` sections.
pub struct OllamaClientBlocking {
    config: LlmConfig,
}

impl OllamaClientBlocking {
    pub fn new(config: LlmConfig) -> Self {
        Self { config }
    }

    /// Run a blocking HTTP operation on a thread outside any tokio context.
    fn run_blocking<T, F>(&self, op: F) -> Result<T, LlmError>
    where
        T: Send + 'static,
        F: FnOnce(reqwest::blocking::Client) -> Result<T, LlmError> + Send + 'static,
    {
        let timeout = Duration::from_secs(self.config.timeout_secs);
        std::thread::spawn(move || {
            let client = reqwest::blocking::Client::builder()
                .timeout(timeout)
                .build()
                .unwrap_or_else(|_| reqwest::blocking::Client::new());
            op(client)
        })
        .join()
        .map_err(|_| LlmError::NotReachable("blocking LLM thread panicked".into()))?
    }

    pub fn is_alive(&self) -> bool {
        let url = format!("{}/api/tags", self.config.base_url);
        self.run_blocking(move |http| {
            http.get(url).send()?;
            Ok(())
        })
        .is_ok()
    }

    pub fn list_models(&self) -> Result<Vec<ModelInfo>, LlmError> {
        let url = format!("{}/api/tags", self.config.base_url);
        let resp: ModelsResponse =
            self.run_blocking(move |http| Ok(http.get(url).send()?.json()?))?;
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
        let url = format!("{}/api/generate", self.config.base_url);

        self.run_blocking(move |http| Ok(http.post(url).json(&req).send()?.json()?))
    }

    pub fn config(&self) -> &LlmConfig {
        &self.config
    }
}
