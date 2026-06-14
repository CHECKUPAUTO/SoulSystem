//! OpenAI provider — communicates with the OpenAI /v1/chat/completions endpoint.
//!
//! Supports both streaming (SSE) and non-streaming modes.

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

pub struct OpenAIProvider {
    name: String,
    config: ProviderConfig,
    client: &'static Client,
}

impl OpenAIProvider {
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
            .unwrap_or_else(|| "gpt-5".to_string())
    }

    fn auth_header(&self) -> Option<(&str, String)> {
        self.config
            .api_key
            .as_ref()
            .map(|k| ("Authorization", format!("Bearer {k}")))
    }
}

#[async_trait]
impl Provider for OpenAIProvider {
    fn name(&self) -> &str {
        &self.name
    }
    fn config(&self) -> &ProviderConfig {
        &self.config
    }

    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse, ProviderError> {
        let model = self.resolve_model(req.model.as_deref());
        let url = format!(
            "{}/chat/completions",
            self.config.base_url.trim_end_matches('/')
        );

        let messages: Vec<Value> = req
            .messages
            .iter()
            .map(|m| {
                json!({
                    "role": m.role,
                    "content": m.content,
                })
            })
            .collect();

        let body = json!({
            "model": model,
            "messages": messages,
            "temperature": req.temperature,
            "max_tokens": req.max_tokens,
        });

        debug!(url = %url, model = %model, "OpenAI chat request");

        let mut req_builder = self
            .client
            .post(&url)
            .timeout(std::time::Duration::from_secs(self.config.timeout_secs))
            .json(&body);

        if let Some((key, val)) = self.auth_header() {
            req_builder = req_builder.header(key, val);
        }

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
        let choice = value["choices"][0].clone();
        let message = choice["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let usage = value.get("usage").map(|u| TokenUsage {
            prompt_tokens: u["prompt_tokens"].as_u64().unwrap_or(0) as u32,
            completion_tokens: u["completion_tokens"].as_u64().unwrap_or(0) as u32,
            total_tokens: u["total_tokens"].as_u64().unwrap_or(0) as u32,
        });

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
        let url = format!(
            "{}/chat/completions",
            self.config.base_url.trim_end_matches('/')
        );

        let messages: Vec<Value> = req
            .messages
            .iter()
            .map(|m| {
                json!({
                    "role": m.role,
                    "content": m.content,
                })
            })
            .collect();

        let body = json!({
            "model": model,
            "messages": messages,
            "temperature": req.temperature,
            "max_tokens": req.max_tokens,
            "stream": true,
            "stream_options": { "include_usage": false },
        });

        debug!(url = %url, model = %model, "OpenAI streaming chat request");

        let client = self.client.clone();
        let auth = self.auth_header().map(|(k, v)| (k.to_string(), v));
        let timeout = self.config.timeout_secs;

        let stream = async_stream::stream! {
            let mut req_builder = client
                .post(&url)
                .timeout(std::time::Duration::from_secs(timeout))
                .json(&body);

            if let Some((key, val)) = &auth {
                req_builder = req_builder.header(key.as_str(), val);
            }

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
                        // SSE format: "data: {...}\n\n"
                        while let Some(pos) = buffer.windows(2).position(|w| w == b"\n\n") {
                            let line: Vec<u8> = buffer.drain(..=pos + 1).collect();
                            let text = String::from_utf8_lossy(&line);
                            for l in text.lines() {
                                let l = l.trim();
                                if let Some(data) = l.strip_prefix("data: ") {
                                    if data == "[DONE]" { return; }
                                    match serde_json::from_str::<Value>(data) {
                                        Ok(delta) => {
                                            if let Some(content) = delta["choices"][0]["delta"]["content"].as_str() {
                                                let finish = delta["choices"][0]["finish_reason"].as_str();
                                                yield Ok(ChatDelta {
                                                    content: content.to_string(),
                                                    finish_reason: finish.map(|s| s.to_string()),
                                                });
                                                if finish.is_some() && finish != Some("null") {
                                                    return;
                                                }
                                            }
                                        }
                                        Err(e) => warn!(line = %data, err = %e, "OpenAI SSE parse error"),
                                    }
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
        let model = self.resolve_model(req.model.as_deref());
        let url = format!("{}/completions", self.config.base_url.trim_end_matches('/'));

        let body = json!({
            "model": model,
            "prompt": req.prompt,
            "temperature": req.temperature,
            "max_tokens": req.max_tokens,
        });

        let mut req_builder = self
            .client
            .post(&url)
            .timeout(std::time::Duration::from_secs(self.config.timeout_secs))
            .json(&body);

        if let Some((key, val)) = self.auth_header() {
            req_builder = req_builder.header(key, val);
        }

        let resp = req_builder.send().await?;
        let s = resp.status();
        if !s.is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            return Err(ProviderError::Upstream {
                status: s.as_u16(),
                body: body_text,
            });
        }

        let value: Value = resp.json().await?;
        let text = value["choices"][0]["text"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let usage = value.get("usage").map(|u| TokenUsage {
            prompt_tokens: u["prompt_tokens"].as_u64().unwrap_or(0) as u32,
            completion_tokens: u["completion_tokens"].as_u64().unwrap_or(0) as u32,
            total_tokens: u["total_tokens"].as_u64().unwrap_or(0) as u32,
        });

        Ok(CompletionResponse {
            model: value
                .get("model")
                .and_then(|m| m.as_str())
                .unwrap_or(&model)
                .to_string(),
            text,
            usage,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_model_strips_prefix() {
        let cfg = ProviderConfig {
            provider_type: "openai".into(),
            base_url: "https://api.openai.com/v1".into(),
            api_key: Some("sk-test".into()),
            models: vec!["gpt-5".into()],
            default_model: Some("gpt-5".into()),
            timeout_secs: 60,
            ..Default::default()
        };
        let p = OpenAIProvider::new("openai".into(), cfg);
        assert_eq!(p.resolve_model(Some("openai/gpt-5")), "gpt-5");
        assert_eq!(p.resolve_model(None), "gpt-5");
    }
}
