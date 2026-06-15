//! Ollama provider — communicates with local Ollama instance via its REST API.
//!
//! Endpoint: POST /api/chat (streaming) and POST /api/generate (completion).
//! Reference: https://github.com/ollama/ollama/blob/main/docs/api.md

use async_trait::async_trait;
use futures::stream::BoxStream;
use futures::StreamExt;
use reqwest::Client;
use serde_json::{json, Value};
use tracing::{debug, warn};

use crate::provider::{
    ChatDelta, ChatMessage, ChatRequest, ChatResponse, CompletionRequest, CompletionResponse,
    Provider, ProviderConfig, ProviderError,
};

pub struct OllamaProvider {
    name: String,
    config: ProviderConfig,
    client: &'static Client,
}

impl OllamaProvider {
    pub fn new(name: String, config: ProviderConfig) -> Self {
        Self {
            name,
            config,
            client: soullink_core::http::shared_client(),
        }
    }

    /// Resolve the effective model name — use the requested model, or the
    /// configured default, or the first available model.
    fn resolve_model(&self, requested: Option<&str>) -> String {
        requested
            .and_then(|m| {
                // Strip provider prefix if present: "ollama/foo" → "foo"
                m.split_once('/').map(|(_, rest)| rest).or(Some(m))
            })
            .map(|m| m.to_string())
            .or_else(|| self.config.default_model.clone())
            .or_else(|| self.config.models.first().cloned())
            .unwrap_or_else(|| "deepseek-v4-flash:cloud".to_string())
    }
}

#[async_trait]
impl Provider for OllamaProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn config(&self) -> &ProviderConfig {
        &self.config
    }

    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse, ProviderError> {
        let model = self.resolve_model(req.model.as_deref());
        let url = format!("{}/api/chat", self.config.base_url.trim_end_matches('/'));

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
            "stream": false,
            "options": {
                "temperature": req.temperature,
                "num_predict": req.max_tokens,
            },
        });

        debug!(url = %url, model = %model, "Ollama chat request");

        let resp = self
            .client
            .post(&url)
            .timeout(std::time::Duration::from_secs(self.config.timeout_secs))
            .json(&body)
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            return Err(ProviderError::Upstream {
                status: status.as_u16(),
                body: body_text,
            });
        }

        let value: Value = resp.json().await?;
        let content = value
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();

        Ok(ChatResponse {
            model: value
                .get("model")
                .and_then(|m| m.as_str())
                .unwrap_or(&model)
                .to_string(),
            message: ChatMessage {
                role: "assistant".into(),
                content,
            },
            usage: None, // Ollama doesn't return token counts in /api/chat
        })
    }

    async fn chat_stream(
        &self,
        req: ChatRequest,
    ) -> Result<BoxStream<'static, Result<ChatDelta, ProviderError>>, ProviderError> {
        let model = self.resolve_model(req.model.as_deref());
        let url = format!("{}/api/chat", self.config.base_url.trim_end_matches('/'));

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
            "stream": true,
            "options": {
                "temperature": req.temperature,
                "num_predict": req.max_tokens,
            },
        });

        debug!(url = %url, model = %model, "Ollama streaming chat request");

        let client = self.client.clone();
        let timeout = self.config.timeout_secs;

        let stream = async_stream::stream! {
            let resp = match client
                .post(&url)
                .timeout(std::time::Duration::from_secs(timeout))
                .json(&body)
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    yield Err(ProviderError::Http(e));
                    return;
                }
            };

            let status = resp.status();
            if !status.is_success() {
                let body_text = resp.text().await.unwrap_or_default();
                yield Err(ProviderError::Upstream { status: status.as_u16(), body: body_text });
                return;
            }

            let mut stream = resp.bytes_stream();
            let mut buffer = Vec::new();

            while let Some(chunk_result) = stream.next().await {
                match chunk_result {
                    Ok(chunk) => {
                        buffer.extend_from_slice(&chunk);
                        // Ollama returns NDJSON — each line is a complete JSON object
                        while let Some(pos) = buffer.iter().position(|&b| b == b'\n') {
                            let line: Vec<u8> = buffer.drain(..=pos).collect();
                            let line = String::from_utf8_lossy(&line[..line.len().saturating_sub(1)]);
                            let line = line.trim();

                            if line.is_empty() || line == "{" || line == "}" {
                                continue;
                            }

                            match serde_json::from_str::<Value>(line) {
                                Ok(delta) => {
                                    if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                                        let done = delta.get("done").and_then(|d| d.as_bool()).unwrap_or(false);
                                        yield Ok(ChatDelta {
                                            content: content.to_string(),
                                            finish_reason: if done { Some("stop".into()) } else { None },
                                        });
                                        if done {
                                            return;
                                        }
                                    }
                                }
                                Err(e) => {
                                    warn!(line = %line, err = %e, "Ollama parse error in stream");
                                }
                            }
                        }
                    }
                    Err(e) => {
                        yield Err(ProviderError::Http(e));
                        return;
                    }
                }
            }
        };

        Ok(Box::pin(stream))
    }

    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        let model = self.resolve_model(req.model.as_deref());
        let url = format!(
            "{}/api/generate",
            self.config.base_url.trim_end_matches('/')
        );

        let body = json!({
            "model": model,
            "prompt": req.prompt,
            "stream": false,
            "options": {
                "temperature": req.temperature,
                "num_predict": req.max_tokens,
            },
        });

        let resp = self
            .client
            .post(&url)
            .timeout(std::time::Duration::from_secs(self.config.timeout_secs))
            .json(&body)
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            return Err(ProviderError::Upstream {
                status: status.as_u16(),
                body: body_text,
            });
        }

        let value: Value = resp.json().await?;
        let text = value
            .get("response")
            .and_then(|r| r.as_str())
            .unwrap_or("")
            .to_string();

        Ok(CompletionResponse {
            model: value
                .get("model")
                .and_then(|m| m.as_str())
                .unwrap_or(&model)
                .to_string(),
            text,
            usage: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_model_no_prefix() {
        let cfg = ProviderConfig {
            provider_type: "ollama".into(),
            base_url: "http://localhost:11434".into(),
            api_key: None,
            models: vec!["deepseek-v4-flash:cloud".into()],
            default_model: Some("deepseek-v4-flash:cloud".into()),
            timeout_secs: 30,
            ..Default::default()
        };
        let p = OllamaProvider::new("ollama".into(), cfg);
        assert_eq!(p.resolve_model(Some("llama3:70b")), "llama3:70b");
    }

    #[test]
    fn test_resolve_model_strips_provider_prefix() {
        let cfg = ProviderConfig {
            provider_type: "ollama".into(),
            base_url: "http://localhost:11434".into(),
            api_key: None,
            models: vec![],
            default_model: Some("deepseek-v4-flash:cloud".into()),
            timeout_secs: 30,
            ..Default::default()
        };
        let p = OllamaProvider::new("ollama".into(), cfg);
        assert_eq!(p.resolve_model(Some("ollama/llama3")), "llama3");
    }

    #[test]
    fn test_resolve_model_falls_back_to_default() {
        let cfg = ProviderConfig {
            provider_type: "ollama".into(),
            base_url: "http://localhost:11434".into(),
            api_key: None,
            models: vec![],
            default_model: Some("llama3:70b".into()),
            timeout_secs: 30,
            ..Default::default()
        };
        let p = OllamaProvider::new("ollama".into(), cfg);
        assert_eq!(p.resolve_model(None), "llama3:70b");
    }

    #[test]
    fn test_resolve_model_falls_back_to_hardcoded() {
        let cfg = ProviderConfig {
            provider_type: "ollama".into(),
            base_url: "http://localhost:11434".into(),
            api_key: None,
            models: vec![],
            default_model: None,
            timeout_secs: 30,
            ..Default::default()
        };
        let p = OllamaProvider::new("ollama".into(), cfg);
        assert_eq!(p.resolve_model(None), "deepseek-v4-flash:cloud");
    }
}
