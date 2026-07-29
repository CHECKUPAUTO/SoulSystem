//! # Legacy OllamaClient / ChatSession compatibility shim
//!
//! The new autonomous entity code in `soul_entity` uses `LlmClient`. The older
//! `soul_agent_core` and the historical monolith (`src/main.rs`) still rely on
//! the original `OllamaClient` + `ChatSession` + native tool-calling API.
//! This module preserves that API by wrapping `LlmClient`.

use crate::client::LlmClient;
use crate::provider::{
    ChatMessage as ProviderChatMessage, ChatRole as ProviderChatRole, ToolCall as ProviderToolCall,
    ToolCallFunction as ProviderToolCallFunction, ToolSchema as ProviderToolSchema,
};
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

    /// Chat, discarding the typed error.
    ///
    /// Kept for existing callers. Prefer [`OllamaClient::chat_typed`] anywhere
    /// the *kind* of failure matters: this returns a `String`, so a caller
    /// cannot ask `is_retryable()` or recognise `RetriesExhausted`, and a
    /// strategy layer above it has no way to tell "the provider already backed
    /// off and retried N times" from "one transient blip".
    pub async fn chat(
        &self,
        messages: &[ChatMessage],
        tools: Option<&[ToolSchema]>,
    ) -> std::result::Result<ChatResponse, String> {
        self.chat_typed(messages, tools)
            .await
            .map_err(|e| e.to_string())
    }

    /// Chat, preserving [`crate::LlmError`].
    ///
    /// P1-4 gave `LlmError` an `is_retryable()` classification and a
    /// `RetriesExhausted { attempts, last }` variant so the provider layer and
    /// a strategy layer above it could stop holding divergent opinions about
    /// what is worth retrying. None of that reached the agent loop, and the
    /// reason was here: `chat` stringified the error two layers down, so the
    /// information was destroyed before anything could branch on it. Adding the
    /// branch without this would have been writing a decision against evidence
    /// that had already been thrown away.
    pub async fn chat_typed(
        &self,
        messages: &[ChatMessage],
        tools: Option<&[ToolSchema]>,
    ) -> crate::LlmResult<ChatResponse> {
        let provider_msgs: Vec<ProviderChatMessage> = messages
            .iter()
            .map(|m| {
                let role = match m.role {
                    Role::System => ProviderChatRole::System,
                    Role::User => ProviderChatRole::User,
                    Role::Assistant => ProviderChatRole::Assistant,
                    Role::Tool => ProviderChatRole::Tool,
                };
                let tool_calls: Option<Vec<ProviderToolCall>> = m.tool_calls.as_ref().map(|tcs| {
                    tcs.iter()
                        .map(|tc| ProviderToolCall {
                            id: tc.id.clone(),
                            function: ProviderToolCallFunction {
                                name: tc.function.name.clone(),
                                arguments: tc.function.arguments.clone(),
                            },
                        })
                        .collect()
                });
                ProviderChatMessage {
                    role,
                    content: m.content.clone(),
                    tool_calls,
                    tool_call_id: m.tool_call_id.clone(),
                }
            })
            .collect();

        let provider_tools: Option<Vec<ProviderToolSchema>> = tools.map(|ts| {
            ts.iter()
                .map(|t| ProviderToolSchema {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    parameters: t.parameters.clone(),
                })
                .collect()
        });

        let tools_ref = provider_tools.as_deref();

        match self.client.chat(&provider_msgs, tools_ref).await {
            Ok(resp) => {
                let tool_calls: Option<Vec<ToolCall>> = resp.message.tool_calls.map(|tcs| {
                    tcs.into_iter()
                        .map(|tc| ToolCall {
                            id: tc.id,
                            function: ToolFunction {
                                name: tc.function.name,
                                arguments: tc.function.arguments,
                            },
                        })
                        .collect()
                });
                Ok(ChatResponse {
                    message: AssistantMessage {
                        content: resp.message.content,
                        tool_calls,
                    },
                })
            }
            Err(e) => Err(e),
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
        ToolSchema {
            name: "browser_read".into(),
            description:
                "Navigate a locally controlled Chrome browser and return visible page text".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string" },
                    "cdp_endpoint": { "type": "string" }
                },
                "required": ["url"]
            }),
        },
        ToolSchema {
            name: "mcp_call".into(),
            description: "Call a tool exposed by an MCP WebSocket server".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "server_url": { "type": "string" },
                    "tool": { "type": "string" },
                    "arguments": { "type": "object" }
                },
                "required": ["server_url", "tool"]
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

#[cfg(test)]
mod typed_error_tests {
    use crate::LlmError;

    /// The classification the agent loop needs must be answerable from the
    /// error alone. These are the three cases `ProviderOutcome` maps.
    #[test]
    fn retries_exhausted_is_distinguishable_from_a_single_transient_failure() {
        let transient = LlmError::from_http_status(503, None, "");
        assert!(
            transient.is_retryable(),
            "a 503 is the provider layer's business to retry"
        );

        let exhausted = LlmError::RetriesExhausted {
            attempts: 4,
            last: Box::new(LlmError::from_http_status(503, None, "")),
        };
        assert!(
            !exhausted.is_retryable(),
            "once the provider layer has spent its budget, an outer layer \
             retrying spends a second one on the same dead provider"
        );
        match exhausted {
            LlmError::RetriesExhausted { attempts, .. } => assert_eq!(attempts, 4),
            other => panic!("expected RetriesExhausted, got {other}"),
        }
    }

    #[test]
    fn a_permanent_failure_is_neither_retryable_nor_exhausted() {
        let permanent = LlmError::from_http_status(401, None, "");
        assert!(!permanent.is_retryable());
        assert!(!matches!(permanent, LlmError::RetriesExhausted { .. }));
    }

    /// The whole point of `chat_typed`: `chat` flattens all three of the above
    /// into one `String`, so a caller of `chat` cannot tell them apart. This
    /// pins that the stringifying path really does destroy the distinction, so
    /// nobody "simplifies" `chat_typed` away later.
    #[test]
    fn the_string_returning_path_destroys_the_distinction() {
        let exhausted = LlmError::RetriesExhausted {
            attempts: 4,
            last: Box::new(LlmError::from_http_status(503, None, "")),
        };
        let transient = LlmError::from_http_status(503, None, "");

        // Both become plain text. Whatever the wording, neither string carries
        // a machine-checkable `is_retryable`, which is what a branch needs.
        let a = exhausted.to_string();
        let b = transient.to_string();
        assert!(!a.is_empty() && !b.is_empty());
        // The classification is recoverable from the typed values...
        assert!(!exhausted.is_retryable() && transient.is_retryable());
        // ...and there is no equivalent call available on the strings.
    }
}
