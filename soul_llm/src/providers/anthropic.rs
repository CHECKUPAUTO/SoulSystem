use crate::error::{LlmError, Result};
use crate::http::SendChecked;
use crate::provider::{
    ChatMessage, ChatResponse, ChatResponseMessage, ChatRole, LlmProvider, ToolCall,
    ToolCallFunction, ToolSchema,
};
use crate::types::{
    EmbeddingResult, GenerateRequest, GenerateResult, ModelInfo, StreamChunk, TokenUsage,
};
use async_trait::async_trait;
use futures::stream::{self, BoxStream, StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use zeroize::Zeroizing;

// ── Anthropic wire types ─────────────────────────────────────

#[derive(Serialize)]
struct AnthropicRequest<'a> {
    model: &'a str,
    messages: Vec<AnthropicMessage<'a>>,
    max_tokens: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    stream: bool,
}

#[derive(Serialize)]
struct AnthropicMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContent>,
    usage: AnthropicUsage,
    model: String,
    #[allow(dead_code)]
    stop_reason: Option<String>,
}

#[derive(Deserialize)]
struct AnthropicContent {
    #[serde(rename = "type")]
    content_type: String,
    text: Option<String>,
}

#[derive(Deserialize)]
struct AnthropicUsage {
    input_tokens: usize,
    output_tokens: usize,
}

#[derive(Deserialize)]
struct AnthropicStreamEvent {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(default)]
    delta: Option<AnthropicStreamDelta>,
    #[serde(default)]
    usage: Option<AnthropicUsage>,
}

#[derive(Deserialize)]
struct AnthropicStreamDelta {
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    stop_reason: Option<String>,
}

// ── Provider ─────────────────────────────────────────────────

pub struct AnthropicProvider {
    http: reqwest::Client,
    base_url: String,
    #[allow(dead_code)]
    api_key: Zeroizing<String>,
    default_model: String,
    temperature: f32,
    max_tokens: usize,
    budget: Arc<crate::budget::LlmBudget>,
}

impl AnthropicProvider {
    pub fn new(config: &crate::types::LlmConfig) -> Result<Self> {
        let api_key = config
            .auth_token
            .clone()
            .ok_or_else(|| LlmError::Auth("Anthropic requires an API key (auth_token)".into()))?;

        let mut builder = reqwest::Client::builder()
            .timeout(config.http_timeout)
            .connect_timeout(config.connect_timeout)
            .pool_max_idle_per_host(config.pool_max_idle)
            .pool_idle_timeout(config.pool_idle_timeout);

        let mut headers = reqwest::header::HeaderMap::new();
        let api_key_value = reqwest::header::HeaderValue::from_str(api_key.expose())
            .map_err(|e| LlmError::Auth(format!("Clé API invalide: {e}")))?;
        headers.insert("x-api-key", api_key_value);
        headers.insert(
            "anthropic-version",
            reqwest::header::HeaderValue::from_static("2023-06-01"),
        );
        headers.insert(
            "content-type",
            reqwest::header::HeaderValue::from_static("application/json"),
        );
        builder = builder.default_headers(headers);

        let http = builder.build().unwrap_or_else(|_| reqwest::Client::new());
        let budget = Arc::new(crate::budget::LlmBudget::new(config.clone()));

        Ok(Self {
            http,
            base_url: config.base_url.trim_end_matches('/').to_string(),
            api_key: Zeroizing::new(api_key.expose().to_owned()),
            default_model: config.model.clone(),
            temperature: config.temperature,
            max_tokens: config.max_tokens,
            budget,
        })
    }

    pub fn budget(&self) -> &crate::budget::LlmBudget {
        &self.budget
    }

    fn model(&self, req: &GenerateRequest) -> String {
        req.model
            .clone()
            .unwrap_or_else(|| self.default_model.clone())
    }

    fn temperature(&self, req: &GenerateRequest) -> f32 {
        req.temperature.unwrap_or(self.temperature)
    }

    fn max_tokens(&self, req: &GenerateRequest) -> usize {
        req.max_tokens.unwrap_or(self.max_tokens)
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    fn name(&self) -> &str {
        "anthropic"
    }

    async fn health_check(&self) -> bool {
        // Anthropic n'a pas d'endpoint health dédié, on tente un model list
        self.http
            .get(format!("{}/v1/models", self.base_url))
            .send_checked()
            .await
            .is_ok()
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        // Anthropic n'a pas d'API models list publique — retourner le modèle par défaut
        Ok(vec![ModelInfo {
            name: self.default_model.clone(),
            size: None,
            family: Some("claude".into()),
        }])
    }

    async fn generate(&self, request: &GenerateRequest) -> Result<GenerateResult> {
        let model = self.model(request);

        let messages = vec![AnthropicMessage {
            role: "user",
            content: &request.prompt,
        }];

        let anthropic_req = AnthropicRequest {
            model: &model,
            messages,
            max_tokens: self.max_tokens(request),
            system: request.system.as_deref(),
            temperature: Some(self.temperature(request)),
            stream: false,
        };

        let resp: AnthropicResponse = self
            .http
            .post(format!("{}/v1/messages", self.base_url))
            .json(&anthropic_req)
            .send_checked()
            .await?
            .json()
            .await?;

        let text = resp
            .content
            .into_iter()
            .filter(|c| c.content_type == "text")
            .filter_map(|c| c.text)
            .collect::<Vec<_>>()
            .join("");

        let usage = TokenUsage::new(resp.usage.input_tokens, resp.usage.output_tokens);

        Ok(GenerateResult {
            text,
            usage,
            model: resp.model,
            duration_ms: 0,
        })
    }

    async fn generate_stream(
        &self,
        request: GenerateRequest,
    ) -> Result<BoxStream<'static, Result<StreamChunk>>> {
        let model = self.model(&request);

        let anthropic_req = AnthropicRequest {
            model: &model,
            messages: vec![AnthropicMessage {
                role: "user",
                content: &request.prompt,
            }],
            max_tokens: self.max_tokens(&request),
            system: request.system.as_deref(),
            temperature: Some(self.temperature(&request)),
            stream: true,
        };

        let response = self
            .http
            .post(format!("{}/v1/messages", self.base_url))
            .json(&anthropic_req)
            .send_checked()
            .await?;

        let byte_stream = response.bytes_stream();

        let mapped = byte_stream.then(move |chunk| async move {
            match chunk {
                Ok(bytes) => {
                    let text = String::from_utf8_lossy(&bytes);
                    let mut chunks: Vec<Result<StreamChunk>> = Vec::new();
                    for line in text.lines() {
                        let line = line.trim();
                        if line.is_empty() || !line.starts_with("data: ") {
                            continue;
                        }
                        let data = &line[6..];
                        if let Ok(event) = serde_json::from_str::<AnthropicStreamEvent>(data) {
                            match event.event_type.as_str() {
                                "content_block_delta" => {
                                    let text = event.delta.and_then(|d| d.text).unwrap_or_default();
                                    chunks.push(Ok(StreamChunk {
                                        text,
                                        done: false,
                                        usage: None,
                                    }));
                                }
                                "message_stop" => {
                                    chunks.push(Ok(StreamChunk {
                                        text: String::new(),
                                        done: true,
                                        usage: event.usage.map(|u| {
                                            TokenUsage::new(u.input_tokens, u.output_tokens)
                                        }),
                                    }));
                                }
                                "message_delta" => {
                                    if let Some(u) = event.usage {
                                        chunks.push(Ok(StreamChunk {
                                            text: String::new(),
                                            done: true,
                                            usage: Some(TokenUsage::new(
                                                u.input_tokens,
                                                u.output_tokens,
                                            )),
                                        }));
                                    }
                                }
                                _ => continue,
                            }
                        }
                    }
                    Box::pin(stream::iter(chunks)) as BoxStream<'_, Result<StreamChunk>>
                }
                Err(e) => Box::pin(stream::once(async move {
                    Err(LlmError::Network(e.to_string()))
                })) as BoxStream<'_, Result<StreamChunk>>,
            }
        });

        Ok(Box::pin(mapped.flatten()))
    }

    async fn embed(&self, _text: &str, _model: Option<&str>) -> Result<EmbeddingResult> {
        Err(LlmError::Unsupported(
            "Anthropic does not provide a native embedding API. \
             Use OpenAI or Ollama for embeddings."
                .into(),
        ))
    }

    /// Native tool calling (LOW-008).
    ///
    /// Overrides the trait default, which flattened tool schemas into prose
    /// and then returned `tool_calls: None` unconditionally — leaving a caller
    /// unable to distinguish "the model called no tool" from "this provider
    /// cannot report tool calls at all".
    ///
    /// Anthropic's shape differs from OpenAI's in a way that matters here:
    /// there is no `tool_calls` array. Tool use arrives as `tool_use` blocks
    /// interleaved with `text` blocks in the same `content` list, and a result
    /// goes back as a `tool_result` block inside a *user* message. So this is
    /// a real translation, not a rename of the OpenAI implementation.
    async fn chat(
        &self,
        messages: &[ChatMessage],
        tools: Option<&[ToolSchema]>,
    ) -> Result<ChatResponse> {
        // Anthropic takes the system prompt as a top-level field rather than a
        // message role, so system messages are lifted out of the list.
        let system: Option<String> = messages
            .iter()
            .filter(|m| matches!(m.role, ChatRole::System))
            .map(|m| m.content.clone())
            .reduce(|a, b| format!("{a}\n{b}"));

        let mut anth_msgs: Vec<AnthropicToolMessage> = Vec::new();
        for m in messages.iter() {
            match m.role {
                ChatRole::System => continue,
                ChatRole::Tool => {
                    // A tool result is a user-role message carrying a
                    // tool_result block, not its own role.
                    anth_msgs.push(AnthropicToolMessage {
                        role: "user".to_string(),
                        content: vec![AnthropicContentOut::ToolResult {
                            tool_use_id: m.tool_call_id.clone().unwrap_or_default(),
                            content: m.content.clone(),
                        }],
                    });
                }
                ChatRole::User | ChatRole::Assistant => {
                    let role = if matches!(m.role, ChatRole::User) {
                        "user"
                    } else {
                        "assistant"
                    };
                    let mut blocks: Vec<AnthropicContentOut> = Vec::new();
                    if !m.content.is_empty() {
                        blocks.push(AnthropicContentOut::Text {
                            text: m.content.clone(),
                        });
                    }
                    if let Some(tcs) = &m.tool_calls {
                        for tc in tcs {
                            blocks.push(AnthropicContentOut::ToolUse {
                                id: tc.id.clone(),
                                name: tc.function.name.clone(),
                                // Arguments travel as a JSON string in our
                                // types but as an object on the wire.
                                input: serde_json::from_str(&tc.function.arguments)
                                    .unwrap_or(serde_json::Value::Object(Default::default())),
                            });
                        }
                    }
                    if blocks.is_empty() {
                        blocks.push(AnthropicContentOut::Text {
                            text: String::new(),
                        });
                    }
                    anth_msgs.push(AnthropicToolMessage {
                        role: role.to_string(),
                        content: blocks,
                    });
                }
            }
        }

        let anth_tools: Option<Vec<AnthropicToolOut>> = tools.map(|ts| {
            ts.iter()
                .map(|t| AnthropicToolOut {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    input_schema: t.parameters.clone(),
                })
                .collect()
        });

        let body = AnthropicToolRequest {
            model: self.default_model.clone(),
            messages: anth_msgs,
            max_tokens: DEFAULT_TOOL_CHAT_MAX_TOKENS,
            system,
            tools: anth_tools,
            stream: false,
        };

        let resp: AnthropicToolResponse = self
            .http
            .post(format!("{}/v1/messages", self.base_url))
            .json(&body)
            // Propagated as-is: send_checked already classified the status.
            .send_checked()
            .await?
            .json()
            .await
            .map_err(|e| LlmError::Provider(format!("anthropic chat deserialize: {e}")))?;

        let mut text_parts: Vec<String> = Vec::new();
        let mut tool_calls: Vec<ToolCall> = Vec::new();
        for block in resp.content {
            match block {
                AnthropicContentIn::Text { text } => text_parts.push(text),
                AnthropicContentIn::ToolUse { id, name, input } => tool_calls.push(ToolCall {
                    id,
                    function: ToolCallFunction {
                        name,
                        // Back to the JSON-string form our types use.
                        arguments: input.to_string(),
                    },
                }),
                AnthropicContentIn::Other => {}
            }
        }

        Ok(ChatResponse {
            message: ChatResponseMessage {
                content: (!text_parts.is_empty()).then(|| text_parts.join("")),
                tool_calls: (!tool_calls.is_empty()).then_some(tool_calls),
            },
        })
    }
}

/// Anthropic requires `max_tokens`; the chat path has no per-request override.
const DEFAULT_TOOL_CHAT_MAX_TOKENS: usize = 4096;

// ── Anthropic tool-calling wire types ────────────────────────

#[derive(Serialize)]
struct AnthropicToolRequest {
    model: String,
    messages: Vec<AnthropicToolMessage>,
    max_tokens: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<AnthropicToolOut>>,
    stream: bool,
}

#[derive(Serialize)]
struct AnthropicToolMessage {
    role: String,
    content: Vec<AnthropicContentOut>,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicContentOut {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
    },
}

#[derive(Serialize)]
struct AnthropicToolOut {
    name: String,
    description: String,
    input_schema: serde_json::Value,
}

#[derive(Deserialize)]
struct AnthropicToolResponse {
    content: Vec<AnthropicContentIn>,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicContentIn {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        #[serde(default)]
        input: serde_json::Value,
    },
    #[serde(other)]
    Other,
}
