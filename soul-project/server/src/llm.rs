use anyhow::Result;
use crate::context::ContextPlan;
use futures::Stream;
use std::pin::Pin;
use async_stream::try_stream;
use serde_json::json;

pub enum LlmStreamEvent {
    Delta(String),
    Usage { input_tokens: u32, output_tokens: u32 },
    Done,
}

#[derive(Clone)]
pub struct LlmClient {
    pub provider: String, // "ollama" or "anthropic"
    pub api_url: String,
    pub api_key: Option<String>,
}

impl LlmClient {
    pub fn new(provider: String, api_url: String, api_key: Option<String>) -> Self {
        Self { provider, api_url, api_key }
    }

    pub async fn stream(&self, model: &str, plan: &ContextPlan) -> Result<Pin<Box<dyn Stream<Item = Result<LlmStreamEvent>> + Send>>> {
        let client = reqwest::Client::new();
        let provider = self.provider.clone();
        let api_url = self.api_url.clone();
        let api_key = self.api_key.clone();
        let model = model.to_string();
        let system = plan.system_prompt.clone();
        let messages = plan.messages.clone();

        Ok(Box::pin(try_stream! {
            if provider == "ollama" {
                let mut resp = client.post(format!("{}/api/chat", api_url))
                    .json(&json!({
                        "model": model,
                        "messages": messages,
                        "stream": true,
                    }))
                    .send().await?;

                while let Some(chunk) = resp.chunk().await? {
                    let s = String::from_utf8_lossy(&chunk);
                    for line in s.lines() {
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(line) {
                            if let Some(delta) = val["message"]["content"].as_str() {
                                yield LlmStreamEvent::Delta(delta.to_string());
                            }
                            if val["done"].as_bool().unwrap_or(false) {
                                yield LlmStreamEvent::Done;
                            }
                        }
                    }
                }
            } else if provider == "anthropic" {
                let mut resp = client.post("https://api.anthropic.com/v1/messages")
                    .header("x-api-key", api_key.unwrap_or_default())
                    .header("anthropic-version", "2023-06-01")
                    .json(&json!({
                        "model": model,
                        "system": system,
                        "messages": messages,
                        "max_tokens": 4096,
                        "stream": true,
                    }))
                    .send().await?;

                while let Some(chunk) = resp.chunk().await? {
                    let s = String::from_utf8_lossy(&chunk);
                    for line in s.lines() {
                        if line.starts_with("data: ") {
                            let data = &line[6..];
                            if let Ok(val) = serde_json::from_str::<serde_json::Value>(data) {
                                match val["type"].as_str() {
                                    Some("content_block_delta") => {
                                        if let Some(delta) = val["delta"]["text"].as_str() {
                                            yield LlmStreamEvent::Delta(delta.to_string());
                                        }
                                    }
                                    Some("message_stop") => {
                                        yield LlmStreamEvent::Done;
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            } else {
                yield LlmStreamEvent::Delta("Unknown provider".into());
                yield LlmStreamEvent::Done;
            }
        }))
    }
}
