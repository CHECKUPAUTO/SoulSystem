use crate::error::{LlmError, Result};
use crate::provider::LlmProvider;
use crate::types::{
    EmbeddingResult, GenerateRequest, GenerateResult, ModelInfo, StreamChunk, TokenUsage,
};
use async_trait::async_trait;
use futures::stream::{self, BoxStream, StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use zeroize::Zeroizing;

// ── OpenAI wire types ────────────────────────────────────────

#[derive(Serialize)]
struct OpenAIChatRequest<'a> {
    model: &'a str,
    messages: Vec<OpenAIMessage<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<usize>,
    stream: bool,
}

#[derive(Serialize)]
struct OpenAIMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct OpenAIChatResponse {
    choices: Vec<OpenAIChoice>,
    usage: Option<OpenAIUsage>,
    model: String,
}

#[derive(Deserialize)]
struct OpenAIChoice {
    message: Option<OpenAIChoiceMessage>,
    #[allow(dead_code)]
    delta: Option<OpenAIChoiceDelta>,
}

#[derive(Deserialize)]
struct OpenAIChoiceMessage {
    content: Option<String>,
}

#[derive(Deserialize)]
struct OpenAIChoiceDelta {
    content: Option<String>,
}

#[derive(Deserialize)]
struct OpenAIUsage {
    prompt_tokens: usize,
    completion_tokens: usize,
    #[allow(dead_code)]
    total_tokens: usize,
}

#[derive(Deserialize)]
struct OpenAIStreamResponse {
    choices: Vec<OpenAIStreamChoice>,
    usage: Option<OpenAIUsage>,
}

#[derive(Deserialize)]
struct OpenAIStreamChoice {
    delta: Option<OpenAIChoiceDelta>,
    finish_reason: Option<String>,
}

#[derive(Serialize)]
struct OpenAIEmbedRequest<'a> {
    model: &'a str,
    input: OpenAIEmbedInput<'a>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum OpenAIEmbedInput<'a> {
    Single(&'a str),
    Batch(Vec<&'a str>),
}

#[derive(Deserialize)]
struct OpenAIEmbedResponse {
    data: Vec<OpenAIEmbedData>,
    model: String,
    usage: Option<OpenAIUsage>,
}

#[derive(Deserialize)]
struct OpenAIEmbedData {
    embedding: Vec<f32>,
}

#[derive(Deserialize)]
struct OpenAIModelsResponse {
    data: Vec<OpenAIModelInfo>,
}

#[derive(Deserialize)]
struct OpenAIModelInfo {
    id: String,
}

// ── Provider ─────────────────────────────────────────────────

pub struct OpenAIProvider {
    http: reqwest::Client,
    base_url: String,
    #[allow(dead_code)]
    api_key: Zeroizing<String>,
    default_model: String,
    temperature: f32,
    max_tokens: usize,
    budget: Arc<crate::budget::LlmBudget>,
}

impl OpenAIProvider {
    pub fn new(config: &crate::types::LlmConfig) -> Result<Self> {
        let api_key = config
            .auth_token
            .clone()
            .ok_or_else(|| LlmError::Auth("OpenAI requires an API key (auth_token)".into()))?;

        let mut builder = reqwest::Client::builder()
            .timeout(config.http_timeout)
            .connect_timeout(config.connect_timeout)
            .pool_max_idle_per_host(config.pool_max_idle)
            .pool_idle_timeout(config.pool_idle_timeout);

        let mut headers = reqwest::header::HeaderMap::new();
        if let Ok(v) = reqwest::header::HeaderValue::from_str(&format!("Bearer {api_key}")) {
            headers.insert(reqwest::header::AUTHORIZATION, v);
        }
        builder = builder.default_headers(headers);

        let http = builder.build().unwrap_or_else(|_| reqwest::Client::new());
        let budget = Arc::new(crate::budget::LlmBudget::new(config.clone()));

        Ok(Self {
            http,
            base_url: config.base_url.trim_end_matches('/').to_string(),
            api_key: Zeroizing::new(api_key),
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
        req.model.clone().unwrap_or_else(|| self.default_model.clone())
    }

    fn temperature(&self, req: &GenerateRequest) -> f32 {
        req.temperature.unwrap_or(self.temperature)
    }

    fn max_tokens(&self, req: &GenerateRequest) -> usize {
        req.max_tokens.unwrap_or(self.max_tokens)
    }
}

#[async_trait]
impl LlmProvider for OpenAIProvider {
    fn name(&self) -> &str {
        "openai"
    }

    async fn health_check(&self) -> bool {
        self.http
            .get(format!("{}/v1/models", self.base_url))
            .send()
            .await
            .is_ok()
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        let resp: OpenAIModelsResponse = self
            .http
            .get(format!("{}/v1/models", self.base_url))
            .send()
            .await?
            .json()
            .await?;

        Ok(resp
            .data
            .into_iter()
            .map(|m| ModelInfo {
                name: m.id,
                size: None,
                family: None,
            })
            .collect())
    }

    async fn generate(&self, request: &GenerateRequest) -> Result<GenerateResult> {
        let model = self.model(request);

        let mut messages = Vec::new();
        if let Some(ref sys) = request.system {
            messages.push(OpenAIMessage {
                role: "system",
                content: sys,
            });
        }
        messages.push(OpenAIMessage {
            role: "user",
            content: &request.prompt,
        });

        let openai_req = OpenAIChatRequest {
            model: &model,
            messages,
            temperature: Some(self.temperature(request)),
            max_tokens: Some(self.max_tokens(request)),
            stream: false,
        };

        let resp: OpenAIChatResponse = self
            .http
            .post(format!("{}/v1/chat/completions", self.base_url))
            .json(&openai_req)
            .send()
            .await?
            .json()
            .await?;

        let text = resp
            .choices
            .first()
            .and_then(|c| c.message.as_ref())
            .and_then(|m| m.content.clone())
            .unwrap_or_default();

        let usage = resp
            .usage
            .map(|u| TokenUsage::new(u.prompt_tokens, u.completion_tokens))
            .unwrap_or_default();

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

        let mut messages = Vec::new();
        if let Some(ref sys) = request.system {
            messages.push(OpenAIMessage {
                role: "system",
                content: sys,
            });
        }
        messages.push(OpenAIMessage {
            role: "user",
            content: &request.prompt,
        });

        let openai_req = OpenAIChatRequest {
            model: &model,
            messages,
            temperature: Some(self.temperature(&request)),
            max_tokens: Some(self.max_tokens(&request)),
            stream: true,
        };

        let response = self
            .http
            .post(format!("{}/v1/chat/completions", self.base_url))
            .json(&openai_req)
            .send()
            .await?;

        let byte_stream = response.bytes_stream();

        let mapped = byte_stream.then(move |chunk| {
            async move {
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
                            if data == "[DONE]" {
                                chunks.push(Ok(StreamChunk {
                                    text: String::new(),
                                    done: true,
                                    usage: None,
                                }));
                                continue;
                            }
                            if let Ok(resp) = serde_json::from_str::<OpenAIStreamResponse>(data) {
                                let text = resp
                                    .choices
                                    .first()
                                    .and_then(|c| c.delta.as_ref())
                                    .and_then(|d| d.content.clone())
                                    .unwrap_or_default();
                                let done = resp
                                    .choices
                                    .first()
                                    .map(|c| c.finish_reason.is_some())
                                    .unwrap_or(false);
                                let usage = resp.usage.map(|u| {
                                    TokenUsage::new(u.prompt_tokens, u.completion_tokens)
                                });
                                chunks.push(Ok(StreamChunk { text, done, usage }));
                            }
                        }
                        Box::pin(stream::iter(chunks)) as BoxStream<'_, Result<StreamChunk>>
                    }
                    Err(e) => Box::pin(stream::once(async move { Err(LlmError::Network(e.to_string())) }))
                        as BoxStream<'_, Result<StreamChunk>>,
                }
            }
        });

        Ok(Box::pin(mapped.flatten()))
    }

    async fn embed(&self, text: &str, model: Option<&str>) -> Result<EmbeddingResult> {
        let model = model.unwrap_or(&self.default_model);
        let req = OpenAIEmbedRequest {
            model,
            input: OpenAIEmbedInput::Single(text),
        };

        let resp: OpenAIEmbedResponse = self
            .http
            .post(format!("{}/v1/embeddings", self.base_url))
            .json(&req)
            .send()
            .await?
            .json()
            .await?;

        let data = resp
            .data
            .into_iter()
            .next()
            .ok_or_else(|| LlmError::Provider("no embedding returned".into()))?;

        let usage = resp
            .usage
            .map(|u| TokenUsage::new(u.prompt_tokens, u.completion_tokens));

        Ok(EmbeddingResult {
            embedding: data.embedding,
            model: resp.model,
            usage,
        })
    }

    async fn embed_batch<'a>(
        &'a self,
        texts: &[&str],
        model: Option<&str>,
    ) -> Result<Vec<EmbeddingResult>> {
        let model = model.unwrap_or(&self.default_model);
        let req = OpenAIEmbedRequest {
            model,
            input: OpenAIEmbedInput::Batch(texts.to_vec()),
        };

        let resp: OpenAIEmbedResponse = self
            .http
            .post(format!("{}/v1/embeddings", self.base_url))
            .json(&req)
            .send()
            .await?
            .json()
            .await?;

        let usage = resp
            .usage
            .map(|u| TokenUsage::new(u.prompt_tokens, u.completion_tokens));

        Ok(resp
            .data
            .into_iter()
            .map(|d| EmbeddingResult {
                embedding: d.embedding,
                model: resp.model.clone(),
                usage: usage.clone(),
            })
            .collect())
    }
}
