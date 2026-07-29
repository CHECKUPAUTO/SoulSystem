use crate::error::{LlmError, Result};
use crate::http::SendChecked;
use crate::provider::LlmProvider;
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
}
