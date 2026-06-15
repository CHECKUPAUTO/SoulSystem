//! # Legacy OllamaClient / ChatSession compatibility shim
//!
//! The new autonomous entity code in `soul_entity` uses `LlmClient`. The older
//! `soul_agent_core` and the historical monolith (`src/main.rs`) still rely on
//! the original `OllamaClient` + `ChatSession` + native tool-calling API.
//! This module preserves that API by wrapping `LlmClient`.

use crate::client::LlmClient;
use crate::types::LlmConfig;
use serde::{Deserialize, Serialize};

// ── Message / Role ───────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
        }
    }
}

// ── Tool schema / call ─────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub function: ToolFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFunction {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantMessage {
    pub content: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub message: AssistantMessage,
}

// ── ChatSession ──────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct ChatSession {
    pub messages: Vec<ChatMessage>,
    pub max_context_chars: usize,
}

impl ChatSession {
    pub fn with_max_context(system_prompt: &str, max_context_chars: usize) -> Self {
        Self {
            messages: vec![ChatMessage::system(system_prompt)],
            max_context_chars,
        }
    }

    pub fn clear(&mut self) {
        // Keep the system prompt (first message).
        if !self.messages.is_empty() {
            let system = self.messages.remove(0);
            self.messages.clear();
            self.messages.push(system);
        }
    }

    pub fn add_user_message(&mut self, content: &str) {
        self.messages.push(ChatMessage::user(content));
    }

    pub fn add_assistant_message(&mut self, content: &str) {
        self.messages.push(ChatMessage::assistant(content));
    }

    pub fn add_assistant_with_tools(&mut self, content: Option<&str>, tool_calls: Vec<ToolCall>) {
        self.messages.push(ChatMessage {
            role: Role::Assistant,
            content: content.unwrap_or_default().to_string(),
            tool_calls: Some(tool_calls),
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

    pub fn history_summary(&self) -> String {
        format!("{} messages", self.messages.len())
    }
}

// ── OllamaClient wrapper ─────────────────────────────────────

#[derive(Clone)]
pub struct OllamaClient {
    client: LlmClient,
}

impl OllamaClient {
    pub fn new(config: LlmConfig) -> Self {
        Self {
            client: LlmClient::new(config).expect("failed to create LlmClient"),
        }
    }

    pub fn with_client(client: LlmClient) -> Self {
        Self { client }
    }

    pub fn config(&self) -> &LlmConfig {
        self.client.config()
    }

    pub async fn chat(
        &self,
        messages: &[ChatMessage],
        _tools: Option<&[ToolSchema]>,
    ) -> std::result::Result<ChatResponse, String> {
        // Convert tool-style messages to a plain prompt; the new provider API
        // is generate-only. We serialize the conversation into the prompt.
        let prompt = messages_to_prompt(messages);

        match self.client.generate_text(&prompt).await {
            Ok(result) => Ok(ChatResponse {
                message: AssistantMessage {
                    content: Some(result),
                    tool_calls: None,
                },
            }),
            Err(e) => Err(e.to_string()),
        }
    }

    pub async fn generate(&self, prompt: &str) -> std::result::Result<ChatResponse, String> {
        match self.client.generate_text(prompt).await {
            Ok(result) => Ok(ChatResponse {
                message: AssistantMessage {
                    content: Some(result),
                    tool_calls: None,
                },
            }),
            Err(e) => Err(e.to_string()),
        }
    }
}

fn messages_to_prompt(messages: &[ChatMessage]) -> String {
    let mut out = String::new();
    for m in messages {
        match m.role {
            Role::System => out.push_str("System: "),
            Role::User => out.push_str("User: "),
            Role::Assistant => out.push_str("Assistant: "),
            Role::Tool => out.push_str("Tool result: "),
        }
        out.push_str(&m.content);
        if let Some(tools) = &m.tool_calls {
            for tc in tools {
                out.push_str(&format!(
                    "\n[tool call {}({})]",
                    tc.function.name, tc.function.arguments
                ));
            }
        }
        out.push('\n');
    }
    out
}

// ── Tool schema builder ──────────────────────────────────────

pub fn build_tool_schemas() -> Vec<ToolSchema> {
    vec![
        ToolSchema {
            name: "execute_shell".into(),
            description: "Execute a safe shell command on the system".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string" }
                },
                "required": ["command"]
            }),
        },
        ToolSchema {
            name: "read_file".into(),
            description: "Read the contents of a file".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                },
                "required": ["path"]
            }),
        },
        ToolSchema {
            name: "write_file".into(),
            description: "Write content to a file".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "content": { "type": "string" }
                },
                "required": ["path", "content"]
            }),
        },
    ]
}

// ── LlmConfig helpers ───────────────────────────────────────

impl LlmConfig {
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }
}
