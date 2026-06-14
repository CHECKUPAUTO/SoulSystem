//! Compatibility chat + tool-calling layer over Ollama's `/api/chat`.
//!
//! The current multi-provider [`crate::LlmClient`] is prompt-completion only.
//! The autonomous ReAct agent (`soul_agent_core`), however, needs a stateful
//! chat session with native tool-calling. This module restores exactly that
//! surface, talking to an Ollama server directly. It is **purely additive** —
//! it does not touch the provider stack — so the multi-provider client and the
//! agent can coexist.

use crate::types::LlmConfig;
use serde::{Deserialize, Serialize};

// ── Chat message types ─────────────────────────────────────────────────

/// Role of a chat message (serialised lowercase for the Ollama API).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// A single message in a chat conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

/// A tool call requested by the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    #[serde(default = "default_call_id")]
    pub id: String,
    #[serde(rename = "type", default = "default_call_type")]
    pub call_type: String,
    pub function: FunctionCall,
}

fn default_call_id() -> String {
    uuid_like()
}
fn default_call_type() -> String {
    "function".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    /// JSON-encoded arguments (Ollama may send an object or a string).
    #[serde(deserialize_with = "de_arguments", default)]
    pub arguments: String,
}

/// A tool the model may call.
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

/// Result of a non-streaming `/api/generate` call.
#[derive(Debug, Clone, Deserialize)]
pub struct GenerateResponse {
    pub response: String,
    #[serde(default)]
    pub done: bool,
}

/// Result of a non-streaming `/api/chat` call.
#[derive(Debug, Clone, Deserialize)]
pub struct ChatResponse {
    pub message: ChatResponseMessage,
    #[serde(default)]
    pub done: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChatResponseMessage {
    #[serde(default)]
    pub role: String,
    pub content: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
}

// ── Chat session (stateful conversation manager) ───────────────────────

/// Maintains an ordered conversation around a fixed system prompt, with tool
/// calls and tool results. The agent's reason→act loop drives it across turns.
#[derive(Debug, Clone)]
pub struct ChatSession {
    pub messages: Vec<ChatMessage>,
    pub max_context_chars: usize,
    system_prompt: String,
}

impl ChatSession {
    pub const DEFAULT_MAX_CONTEXT_CHARS: usize = 40_000;

    pub fn new(system_prompt: &str) -> Self {
        Self::with_max_context(system_prompt, Self::DEFAULT_MAX_CONTEXT_CHARS)
    }

    pub fn with_max_context(system_prompt: &str, max_context_chars: usize) -> Self {
        let mut s = Self {
            messages: Vec::new(),
            max_context_chars,
            system_prompt: system_prompt.to_string(),
        };
        if !system_prompt.is_empty() {
            s.messages.push(ChatMessage {
                role: Role::System,
                content: system_prompt.to_string(),
                tool_calls: None,
                tool_call_id: None,
            });
        }
        s
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

    pub fn build_messages(&self) -> Vec<ChatMessage> {
        self.messages.clone()
    }

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
                format!("{role}: {content}")
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

// ── Default tool schemas (mirror soul_tools::dispatch_tool) ─────────────

/// Tool schemas advertised to the model. These mirror the tools handled by
/// `soul_tools::dispatch_tool`, so the model only requests calls the agent can
/// actually execute.
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
            json!({"type":"object","properties":{"command":{"type":"string"}},"required":["command"]}),
        ),
        tool(
            "read_file",
            "Read a text file, optionally a line range.",
            json!({"type":"object","properties":{"path":{"type":"string"},"start_line":{"type":"integer"},"num_lines":{"type":"integer"}},"required":["path"]}),
        ),
        tool(
            "write_file",
            "Write or append text content to a file.",
            json!({"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"},"mode":{"type":"string","enum":["overwrite","append"]}},"required":["path","content"]}),
        ),
        tool(
            "patch_file",
            "Replace an exact text fragment within a file.",
            json!({"type":"object","properties":{"path":{"type":"string"},"old_text":{"type":"string"},"new_text":{"type":"string"}},"required":["path","old_text","new_text"]}),
        ),
        tool(
            "list_directory",
            "List the entries of a directory.",
            json!({"type":"object","properties":{"path":{"type":"string"}}}),
        ),
        tool(
            "search_files",
            "Find files matching a name pattern under a path.",
            json!({"type":"object","properties":{"pattern":{"type":"string"},"path":{"type":"string"}},"required":["pattern"]}),
        ),
        tool(
            "grep_content",
            "Search file contents for a substring.",
            json!({"type":"object","properties":{"pattern":{"type":"string"},"path":{"type":"string"},"file_pattern":{"type":"string"}},"required":["pattern"]}),
        ),
    ]
}

// ── Ollama HTTP client ─────────────────────────────────────────────────

/// Minimal Ollama client supporting `generate` and tool-calling `chat`.
pub struct OllamaClient {
    config: LlmConfig,
    http: reqwest::Client,
}

impl OllamaClient {
    pub fn new(config: LlmConfig) -> Self {
        let http = reqwest::Client::builder()
            .timeout(config.http_timeout)
            .build()
            .unwrap_or_default();
        Self { config, http }
    }

    pub fn config(&self) -> &LlmConfig {
        &self.config
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.config.base_url.trim_end_matches('/'), path)
    }

    pub async fn is_alive(&self) -> bool {
        self.http.get(self.url("/api/tags")).send().await.is_ok()
    }

    pub async fn generate(&self, prompt: &str) -> Result<GenerateResponse, String> {
        let body = serde_json::json!({
            "model": self.config.model,
            "prompt": prompt,
            "stream": false,
            "options": { "temperature": self.config.temperature, "num_predict": self.config.max_tokens },
        });
        self.http
            .post(self.url("/api/generate"))
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .json::<GenerateResponse>()
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn chat(
        &self,
        messages: &[ChatMessage],
        tools: Option<&[ToolSchema]>,
    ) -> Result<ChatResponse, String> {
        let body = ChatRequest {
            model: self.config.model.clone(),
            messages: messages.to_vec(),
            stream: false,
            tools: tools.map(|t| t.to_vec()),
            options: GenerateOptions {
                temperature: self.config.temperature,
                num_predict: self.config.max_tokens,
            },
        };
        self.http
            .post(self.url("/api/chat"))
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .json::<ChatResponse>()
            .await
            .map_err(|e| e.to_string())
    }
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ToolSchema>>,
    options: GenerateOptions,
}

#[derive(Serialize)]
struct GenerateOptions {
    temperature: f32,
    num_predict: usize,
}

// ── Helpers ────────────────────────────────────────────────────────────

/// Ollama sometimes returns `arguments` as a JSON object rather than a string;
/// normalise both to a JSON string the agent can parse.
fn de_arguments<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v = serde_json::Value::deserialize(deserializer)?;
    Ok(match v {
        serde_json::Value::String(s) => s,
        other => other.to_string(),
    })
}

/// Lightweight unique id without pulling a uuid dependency into this module.
fn uuid_like() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("call_{nanos:x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_keeps_system_on_clear() {
        let mut s = ChatSession::new("sys");
        s.add_user_message("hi");
        s.add_assistant_message("yo");
        assert_eq!(s.messages.len(), 3);
        s.clear();
        assert_eq!(s.messages.len(), 1);
        assert_eq!(s.messages[0].role, Role::System);
    }

    #[test]
    fn tool_schemas_cover_dispatch() {
        let names: Vec<_> = build_tool_schemas()
            .iter()
            .map(|t| t.function.name.clone())
            .collect();
        for n in [
            "execute_shell",
            "read_file",
            "write_file",
            "patch_file",
            "list_directory",
            "search_files",
            "grep_content",
        ] {
            assert!(names.contains(&n.to_string()), "missing schema {n}");
        }
    }

    #[test]
    fn arguments_object_normalised_to_string() {
        let tc: ToolCall = serde_json::from_str(
            r#"{"function":{"name":"execute_shell","arguments":{"command":"ls"}}}"#,
        )
        .unwrap();
        assert!(tc.function.arguments.contains("command"));
        assert_eq!(tc.function.name, "execute_shell");
    }

    #[test]
    fn history_summary_is_compact() {
        let mut s = ChatSession::new("sys");
        s.add_user_message("question");
        let summary = s.history_summary();
        assert!(summary.contains("system:"));
        assert!(summary.contains("user: question"));
    }
}
