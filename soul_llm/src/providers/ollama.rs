use crate::error::{LlmError, Result};
use crate::provider::{ChatMessage, ChatResponse, ChatResponseMessage, ChatRole, LlmProvider, ToolCall, ToolCallFunction, ToolSchema};
use crate::types::{
    EmbeddingResult, GenerateRequest, GenerateResult, ModelInfo, StreamChunk, TokenUsage,
};
use async_trait::async_trait;
use futures::stream::{self, BoxStream, StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// ── Ollama wire types ────────────────────────────────────────

#[derive(Serialize)]
struct OllamaGenerateRequest<'a> {
    model: &'a str,
    prompt: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<&'a str>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<OllamaOptions>,
}

#[derive(Serialize)]
struct OllamaOptions {
    temperature: f32,
    num_predict: usize,
}

#[derive(Deserialize)]
struct OllamaGenerateResponse {
    response: String,
    done: bool,
    #[serde(default)]
    prompt_eval_count: usize,
    #[serde(default)]
    eval_count: usize,
    #[serde(default)]
    total_duration: u64,
}

#[derive(Serialize)]
struct OllamaEmbedRequest<'a> {
    model: &'a str,
    input: Vec<&'a str>,
}

#[derive(Deserialize)]
struct OllamaEmbedResponse {
    embeddings: Vec<Vec<f32>>,
}

#[derive(Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModelInfo>,
}

#[derive(Deserialize)]
struct OllamaModelInfo {
    name: String,
    #[serde(default)]
    size: Option<u64>,
    #[serde(default)]
    family: Option<String>,
}

// ── Provider ─────────────────────────────────────────────────

pub struct OllamaProvider {
    http: reqwest::Client,
    base_url: String,
    default_model: String,
    temperature: f32,
    max_tokens: usize,
    budget: Arc<crate::budget::LlmBudget>,
}

impl OllamaProvider {
    pub fn new(config: &crate::types::LlmConfig) -> Self {
        let mut builder = reqwest::Client::builder()
            .timeout(config.http_timeout)
            .connect_timeout(config.connect_timeout)
            .pool_max_idle_per_host(config.pool_max_idle)
            .pool_idle_timeout(config.pool_idle_timeout);

        if let Some(ref token) = config.auth_token {
            let mut headers = reqwest::header::HeaderMap::new();
            if let Ok(v) = reqwest::header::HeaderValue::from_str(&format!("Bearer {token}")) {
                headers.insert(reqwest::header::AUTHORIZATION, v);
            }
            builder = builder.default_headers(headers);
        }

        let http = builder.build().unwrap_or_else(|_| reqwest::Client::new());
        let budget = Arc::new(crate::budget::LlmBudget::new(config.clone()));

        Self {
            http,
            base_url: config.base_url.clone(),
            default_model: config.model.clone(),
            temperature: config.temperature,
            max_tokens: config.max_tokens,
            budget,
        }
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
impl LlmProvider for OllamaProvider {
    fn name(&self) -> &str {
        "ollama"
    }

    async fn health_check(&self) -> bool {
        self.http
            .get(format!("{}/api/tags", self.base_url))
            .send()
            .await
            .is_ok()
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        let resp: OllamaTagsResponse = self
            .http
            .get(format!("{}/api/tags", self.base_url))
            .send()
            .await?
            .json()
            .await?;

        Ok(resp
            .models
            .into_iter()
            .map(|m| ModelInfo {
                name: m.name,
                size: m.size,
                family: m.family,
            })
            .collect())
    }

    async fn generate(&self, request: &GenerateRequest) -> Result<GenerateResult> {
        let model = self.model(request);
        let ollama_req = OllamaGenerateRequest {
            model: &model,
            prompt: &request.prompt,
            system: request.system.as_deref(),
            stream: false,
            options: Some(OllamaOptions {
                temperature: self.temperature(request),
                num_predict: self.max_tokens(request),
            }),
        };

        let resp: OllamaGenerateResponse = self
            .http
            .post(format!("{}/api/generate", self.base_url))
            .json(&ollama_req)
            .send()
            .await?
            .json()
            .await?;

        let usage = TokenUsage::new(resp.prompt_eval_count, resp.eval_count);
        let duration_ms = resp.total_duration / 1_000_000;

        Ok(GenerateResult {
            text: resp.response,
            usage,
            model: model.to_string(),
            duration_ms,
        })
    }

    async fn generate_stream(
        &self,
        request: GenerateRequest,
    ) -> Result<BoxStream<'static, Result<StreamChunk>>> {
        let model = self.model(&request);
        let ollama_req = OllamaGenerateRequest {
            model: &model,
            prompt: &request.prompt,
            system: request.system.as_deref(),
            stream: true,
            options: Some(OllamaOptions {
                temperature: self.temperature(&request),
                num_predict: self.max_tokens(&request),
            }),
        };

        let response = self
            .http
            .post(format!("{}/api/generate", self.base_url))
            .json(&ollama_req)
            .send()
            .await?;

        let byte_stream = response.bytes_stream();

        let mapped = byte_stream.then(move |chunk| async move {
            match chunk {
                Ok(bytes) => {
                    let text = String::from_utf8_lossy(&bytes);
                    let mut chunks: Vec<Result<StreamChunk>> = Vec::new();
                    for line in text.lines() {
                        if line.trim().is_empty() {
                            continue;
                        }
                        if let Ok(resp) = serde_json::from_str::<OllamaGenerateResponse>(line) {
                            let usage = if resp.done
                                && (resp.prompt_eval_count > 0 || resp.eval_count > 0)
                            {
                                Some(TokenUsage::new(resp.prompt_eval_count, resp.eval_count))
                            } else {
                                None
                            };
                            chunks.push(Ok(StreamChunk {
                                text: resp.response,
                                done: resp.done,
                                usage,
                            }));
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

    async fn chat(
        &self,
        messages: &[ChatMessage],
        tools: Option<&[ToolSchema]>,
    ) -> Result<ChatResponse> {
        #[derive(Serialize)]
        struct OllamaMessage<'a> {
            role: String,
            content: String,
            #[serde(skip_serializing_if = "Option::is_none")]
            tool_calls: Option<Vec<ToolCallOllama<'a>>>,
        }

        #[derive(Serialize)]
        struct ToolCallOllama<'a> {
            function: ToolCallFunctionOllama<'a>,
        }

        #[derive(Serialize)]
        struct ToolCallFunctionOllama<'a> {
            name: &'a str,
            arguments: &'a String,
        }

        #[derive(Serialize)]
        struct OllamaTool {
            #[serde(rename = "type")]
            tool_type: String,
            function: OllamaFunction,
        }

        #[derive(Serialize)]
        struct OllamaFunction {
            name: String,
            description: String,
            parameters: serde_json::Value,
        }

        #[derive(Serialize)]
        struct OllamaChatRequest<'a> {
            model: String,
            messages: Vec<OllamaMessage<'a>>,
            stream: bool,
            #[serde(skip_serializing_if = "Option::is_none")]
            tools: Option<Vec<OllamaTool>>,
        }

        #[derive(Deserialize)]
        struct OllamaChatResponse {
            message: OllamaChatMessage,
        }

        #[derive(Deserialize)]
        struct OllamaChatMessage {
            #[serde(default)]
            content: Option<String>,
            #[serde(default)]
            tool_calls: Option<Vec<OllamaToolCallResp>>,
        }

        #[derive(Deserialize)]
        struct OllamaToolCallResp {
            function: OllamaToolFuncResp,
        }

        #[derive(Deserialize)]
        struct OllamaToolFuncResp {
            name: String,
            #[serde(default)]
            arguments: serde_json::Value,
        }

        let ollama_msgs: Vec<OllamaMessage> = messages.iter().map(|m| {
            let role = match m.role {
                ChatRole::System => "system",
                ChatRole::User => "user",
                ChatRole::Assistant => "assistant",
                ChatRole::Tool => "tool",
            };
            let tool_calls: Option<Vec<ToolCallOllama>> = m.tool_calls.as_ref().map(|tcs| {
                tcs.iter().map(|tc| ToolCallOllama {
                    function: ToolCallFunctionOllama {
                        name: &tc.function.name,
                        arguments: &tc.function.arguments,
                    },
                }).collect()
            });
            OllamaMessage {
                role: role.to_string(),
                content: m.content.clone(),
                tool_calls,
            }
        }).collect();

        let ollama_tools: Option<Vec<OllamaTool>> = tools.map(|ts| {
            ts.iter().map(|t| OllamaTool {
                tool_type: "function".to_string(),
                function: OllamaFunction {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    parameters: t.parameters.clone(),
                },
            }).collect()
        });

        let body = OllamaChatRequest {
            model: self.default_model.clone(),
            messages: ollama_msgs,
            stream: false,
            tools: ollama_tools,
        };

        let resp: OllamaChatResponse = self
            .http
            .post(format!("{}/api/chat", self.base_url))
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError::Provider(format!("ollama chat: {e}")))?
            .json()
            .await
            .map_err(|e| LlmError::Provider(format!("ollama chat deserialize: {e}")))?;

        let tool_calls: Option<Vec<ToolCall>> = resp.message.tool_calls.map(|tcs| {
            tcs.into_iter().map(|tc| ToolCall {
                id: uuid_like(),
                function: ToolCallFunction {
                    name: tc.function.name,
                    arguments: match tc.function.arguments {
                        serde_json::Value::String(s) => s,
                        other => other.to_string(),
                    },
                },
            }).collect()
        });

        Ok(ChatResponse {
            message: ChatResponseMessage {
                content: resp.message.content,
                tool_calls,
            },
        })
    }

    async fn embed(&self, text: &str, model: Option<&str>) -> Result<EmbeddingResult> {
        let model = model.unwrap_or(&self.default_model);
        let req = OllamaEmbedRequest {
            model,
            input: vec![text],
        };

        let resp: OllamaEmbedResponse = self
            .http
            .post(format!("{}/api/embed", self.base_url))
            .json(&req)
            .send()
            .await?
            .json()
            .await?;

        let embedding = resp
            .embeddings
            .into_iter()
            .next()
            .ok_or_else(|| LlmError::Provider("no embedding returned".into()))?;

        Ok(EmbeddingResult {
            embedding,
            model: model.to_string(),
            usage: None,
        })
    }

    async fn embed_batch<'a>(
        &'a self,
        texts: &[&str],
        model: Option<&str>,
    ) -> Result<Vec<EmbeddingResult>> {
        let model = model.unwrap_or(&self.default_model);
        let req = OllamaEmbedRequest {
            model,
            input: texts.to_vec(),
        };

        let resp: OllamaEmbedResponse = self
            .http
            .post(format!("{}/api/embed", self.base_url))
            .json(&req)
            .send()
            .await?
            .json()
            .await?;

        Ok(resp
            .embeddings
            .into_iter()
            .map(|e| EmbeddingResult {
                embedding: e,
                model: model.to_string(),
                usage: None,
            })
            .collect())
    }
}

fn uuid_like() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let rand: u64 = (nanos as u64).wrapping_mul(0x9E6C63D0676A9A99);
    format!("call_{:016x}", rand)
}
