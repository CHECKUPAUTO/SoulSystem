//! Anthropic provider — communicates with the Claude Messages API
//! (`POST /v1/messages`). Supports both streaming (SSE) and non-streaming modes.
//!
//! Wire-level notes (the Messages API differs from the OpenAI shape):
//!   * Auth is the `x-api-key` header plus a required `anthropic-version`.
//!   * `system` is a top-level field, not a message — any `system`-role entries
//!     in the request are hoisted out of `messages` into it.
//!   * `max_tokens` is required.
//!   * `temperature` is intentionally NOT sent: the current Claude models
//!     (Opus 4.7/4.8, Fable 5) reject sampling parameters with a 400, and the
//!     gateway can't know the target model's family up front. Steer behaviour
//!     via the system prompt instead.
//!   * Streaming deltas arrive as `content_block_delta` events carrying a
//!     `text_delta`, terminated by `message_stop`.

use async_trait::async_trait;
use futures::stream::BoxStream;
use futures::StreamExt;
use reqwest::Client;
use serde_json::{json, Value};
use tracing::{debug, warn};

use crate::provider::{
    ChatDelta, ChatMessage, ChatRequest, ChatResponse, CompletionRequest, CompletionResponse,
    Provider, ProviderConfig, ProviderError, TokenUsage,
};

/// API version pinned for the Messages API. See the Anthropic docs.
const ANTHROPIC_VERSION: &str = "2023-06-01";

pub struct AnthropicProvider {
    name: String,
    config: ProviderConfig,
    client: &'static Client,
}

impl AnthropicProvider {
    pub fn new(name: String, config: ProviderConfig) -> Self {
        Self {
            name,
            config,
            client: soullink_core::http::shared_client(),
        }
    }

    fn resolve_model(&self, requested: Option<&str>) -> String {
        requested
            .and_then(|m| m.split_once('/').map(|(_, rest)| rest).or(Some(m)))
            .map(|m| m.to_string())
            .or_else(|| self.config.default_model.clone())
            .or_else(|| self.config.models.first().cloned())
            .unwrap_or_else(|| "claude-opus-4-8".to_string())
    }

    /// `{base_url}/v1/messages`, tolerating a base URL that already ends in
    /// `/v1` (or a trailing slash).
    fn messages_url(&self) -> String {
        let base = self.config.base_url.trim_end_matches('/');
        let base = base.strip_suffix("/v1").unwrap_or(base);
        format!("{base}/v1/messages")
    }

    /// Apply the auth + version headers shared by every request.
    fn with_headers(&self, mut rb: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        rb = rb.header("anthropic-version", ANTHROPIC_VERSION);
        if let Some(key) = self.config.api_key.as_ref() {
            rb = rb.header("x-api-key", key.expose());
        }
        rb
    }

    /// Split the conversation into `(system_prompt, messages)`, since the
    /// Messages API carries the system prompt out-of-band. Consecutive system
    /// turns are joined with newlines.
    fn split_system(messages: &[ChatMessage]) -> (Option<String>, Vec<Value>) {
        let mut system_parts = Vec::new();
        let mut turns = Vec::new();
        for m in messages {
            if m.role == "system" {
                system_parts.push(m.content.clone());
            } else {
                turns.push(json!({ "role": m.role, "content": m.content }));
            }
        }
        let system = if system_parts.is_empty() {
            None
        } else {
            Some(system_parts.join("\n"))
        };
        (system, turns)
    }

    fn build_body(&self, model: &str, req: &ChatRequest, stream: bool) -> Value {
        let (system, messages) = Self::split_system(&req.messages);
        let mut body = json!({
            "model": model,
            "messages": messages,
            "max_tokens": req.max_tokens,
            "stream": stream,
        });
        if let Some(system) = system {
            body["system"] = json!(system);
        }
        body
    }
}

/// Pull the concatenated text out of a non-streaming Messages response, whose
/// `content` is an array of typed blocks.
fn extract_text(value: &Value) -> String {
    value["content"]
        .as_array()
        .map(|blocks| {
            blocks
                .iter()
                .filter(|b| b["type"] == "text")
                .filter_map(|b| b["text"].as_str())
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default()
}

/// Map Anthropic's `usage` (`input_tokens`/`output_tokens`, no total) onto the
/// gateway's `TokenUsage`.
fn extract_usage(value: &Value) -> Option<TokenUsage> {
    value.get("usage").map(|u| {
        let prompt = u["input_tokens"].as_u64().unwrap_or(0) as u32;
        let completion = u["output_tokens"].as_u64().unwrap_or(0) as u32;
        TokenUsage {
            prompt_tokens: prompt,
            completion_tokens: completion,
            total_tokens: prompt + completion,
        }
    })
}

#[async_trait]
impl Provider for AnthropicProvider {
    fn name(&self) -> &str {
        &self.name
    }
    fn config(&self) -> &ProviderConfig {
        &self.config
    }

    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse, ProviderError> {
        let model = self.resolve_model(req.model.as_deref());
        let url = self.messages_url();
        let body = self.build_body(&model, &req, false);

        debug!(url = %url, model = %model, "Anthropic chat request");

        let req_builder = self
            .with_headers(self.client.post(&url))
            .timeout(std::time::Duration::from_secs(self.config.timeout_secs))
            .json(&body);

        let resp = req_builder.send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            return Err(ProviderError::Upstream {
                status: status.as_u16(),
                body: body_text,
            });
        }

        let value: Value = resp.json().await?;
        let message = extract_text(&value);
        let usage = extract_usage(&value);

        Ok(ChatResponse {
            model: value
                .get("model")
                .and_then(|m| m.as_str())
                .unwrap_or(&model)
                .to_string(),
            message: ChatMessage {
                role: "assistant".into(),
                content: message,
            },
            usage,
        })
    }

    async fn chat_stream(
        &self,
        req: ChatRequest,
    ) -> Result<BoxStream<'static, Result<ChatDelta, ProviderError>>, ProviderError> {
        let model = self.resolve_model(req.model.as_deref());
        let url = self.messages_url();
        let body = self.build_body(&model, &req, true);

        debug!(url = %url, model = %model, "Anthropic streaming chat request");

        let req_builder = self
            .with_headers(self.client.post(&url))
            .timeout(std::time::Duration::from_secs(self.config.timeout_secs))
            .json(&body);

        let stream = async_stream::stream! {
            let resp = match req_builder.send().await {
                Ok(r) => r,
                Err(e) => { yield Err(ProviderError::Http(e)); return; }
            };

            let s = resp.status();
            if !s.is_success() {
                let body_text = resp.text().await.unwrap_or_default();
                yield Err(ProviderError::Upstream { status: s.as_u16(), body: body_text });
                return;
            }

            let mut stream = resp.bytes_stream();
            let mut buffer = Vec::new();

            while let Some(chunk_result) = stream.next().await {
                match chunk_result {
                    Ok(chunk) => {
                        buffer.extend_from_slice(&chunk);
                        // SSE frames are separated by a blank line.
                        while let Some(pos) = buffer.windows(2).position(|w| w == b"\n\n") {
                            let frame: Vec<u8> = buffer.drain(..=pos + 1).collect();
                            let text = String::from_utf8_lossy(&frame);
                            for l in text.lines() {
                                let l = l.trim();
                                let Some(data) = l.strip_prefix("data: ") else { continue };
                                let Ok(event) = serde_json::from_str::<Value>(data) else {
                                    warn!(line = %data, "Anthropic SSE parse error");
                                    continue;
                                };
                                match event["type"].as_str() {
                                    Some("content_block_delta") => {
                                        if event["delta"]["type"] == "text_delta" {
                                            if let Some(t) = event["delta"]["text"].as_str() {
                                                yield Ok(ChatDelta {
                                                    content: t.to_string(),
                                                    finish_reason: None,
                                                });
                                            }
                                        }
                                    }
                                    Some("message_delta") => {
                                        if let Some(reason) = event["delta"]["stop_reason"].as_str() {
                                            yield Ok(ChatDelta {
                                                content: String::new(),
                                                finish_reason: Some(reason.to_string()),
                                            });
                                        }
                                    }
                                    Some("message_stop") => return,
                                    _ => {}
                                }
                            }
                        }
                    }
                    Err(e) => { yield Err(ProviderError::Http(e)); return; }
                }
            }
        };

        Ok(Box::pin(stream))
    }

    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        // The Messages API has no text-completion endpoint; wrap the prompt as a
        // single user turn.
        let chat_req = ChatRequest {
            model: req.model.clone(),
            messages: vec![ChatMessage {
                role: "user".into(),
                content: req.prompt,
            }],
            stream: false,
            temperature: req.temperature,
            max_tokens: req.max_tokens,
        };
        let resp = self.chat(chat_req).await?;
        Ok(CompletionResponse {
            model: resp.model,
            text: resp.message.content,
            usage: resp.usage,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider(base_url: &str) -> AnthropicProvider {
        let cfg = ProviderConfig {
            provider_type: "anthropic".into(),
            base_url: base_url.into(),
            api_key: Some("sk-ant-test".into()),
            models: vec!["claude-opus-4-8".into()],
            default_model: Some("claude-opus-4-8".into()),
            timeout_secs: 60,
            ..Default::default()
        };
        AnthropicProvider::new("anthropic".into(), cfg)
    }

    #[test]
    fn resolve_model_strips_prefix_and_defaults() {
        let p = provider("https://api.anthropic.com");
        assert_eq!(
            p.resolve_model(Some("anthropic/claude-haiku-4-5")),
            "claude-haiku-4-5"
        );
        assert_eq!(p.resolve_model(None), "claude-opus-4-8");
    }

    #[test]
    fn messages_url_tolerates_v1_suffix() {
        assert_eq!(
            provider("https://api.anthropic.com").messages_url(),
            "https://api.anthropic.com/v1/messages"
        );
        assert_eq!(
            provider("https://api.anthropic.com/v1").messages_url(),
            "https://api.anthropic.com/v1/messages"
        );
        assert_eq!(
            provider("https://proxy.local/").messages_url(),
            "https://proxy.local/v1/messages"
        );
    }

    #[test]
    fn split_system_hoists_system_turns() {
        let msgs = vec![
            ChatMessage {
                role: "system".into(),
                content: "be terse".into(),
            },
            ChatMessage {
                role: "system".into(),
                content: "use rust".into(),
            },
            ChatMessage {
                role: "user".into(),
                content: "hi".into(),
            },
        ];
        let (system, turns) = AnthropicProvider::split_system(&msgs);
        assert_eq!(system.as_deref(), Some("be terse\nuse rust"));
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0]["role"], "user");
    }

    #[test]
    fn build_body_omits_temperature_and_sets_max_tokens() {
        let p = provider("https://api.anthropic.com");
        let req = ChatRequest {
            model: None,
            messages: vec![
                ChatMessage {
                    role: "system".into(),
                    content: "sys".into(),
                },
                ChatMessage {
                    role: "user".into(),
                    content: "hello".into(),
                },
            ],
            stream: false,
            temperature: 0.7,
            max_tokens: 1024,
        };
        let body = p.build_body("claude-opus-4-8", &req, false);
        assert_eq!(body["max_tokens"], 1024);
        assert_eq!(body["system"], "sys");
        assert!(
            body.get("temperature").is_none(),
            "temperature must not be sent"
        );
        assert_eq!(body["messages"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn extract_text_joins_text_blocks() {
        let value = json!({
            "content": [
                {"type": "text", "text": "Hello "},
                {"type": "tool_use", "name": "x"},
                {"type": "text", "text": "world"}
            ]
        });
        assert_eq!(extract_text(&value), "Hello world");
    }

    #[test]
    fn extract_usage_sums_tokens() {
        let value = json!({ "usage": { "input_tokens": 10, "output_tokens": 5 } });
        let usage = extract_usage(&value).unwrap();
        assert_eq!(usage.prompt_tokens, 10);
        assert_eq!(usage.completion_tokens, 5);
        assert_eq!(usage.total_tokens, 15);
    }
}
