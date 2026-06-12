use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::json;
use tokio::sync::Mutex;
use tracing::{debug, warn};

/// Errors from the LLM client.
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("LLM returned empty response")]
    EmptyResponse,

    #[error("LLM JSON parse error: {0}")]
    JsonParse(String),

    #[error("Circuit breaker is open")]
    CircuitOpen,

    #[error("token budget exceeded: {0}")]
    BudgetExceeded(String),
}

/// Client for Ollama's chat API.
#[derive(Debug, Clone)]
pub struct LlmClient {
    base_url: String,
    model: String,
    http: reqwest::Client,
    inner: Arc<CircuitBreakerInner>,
}

#[derive(Debug)]
struct CircuitBreakerInner {
    consecutive_failures: AtomicUsize,
    last_failure: Mutex<Option<Instant>>,
}

#[derive(Debug, Deserialize)]
struct OllamaResponse {
    message: Option<OllamaMessage>,
    response: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OllamaMessage {
    content: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct TagsResponse {
    models: Vec<TagModel>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct TagModel {
    name: String,
}

impl LlmClient {
    /// Create a new LLM client and verify the Ollama server is reachable.
    ///
    /// # Errors
    /// Returns `LlmError` if the tags endpoint is unreachable.
    pub async fn new(base_url: String, model: String) -> Result<Self, LlmError> {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(600))
            .pool_max_idle_per_host(4)
            .build()?;

        let client = Self {
            base_url,
            model,
            http,
            inner: Arc::new(CircuitBreakerInner {
                consecutive_failures: AtomicUsize::new(0),
                last_failure: Mutex::new(None),
            }),
        };

        // Verify connectivity with retry: 3 attempts, 2s/4s/8s
        let mut last_err = None;
        for i in 0..3 {
            if i > 0 {
                tokio::time::sleep(Duration::from_secs(2u64.pow(i))).await;
            }
            match client.ping().await {
                Ok(()) => {
                    debug!(
                        "Ollama reachable at {}, model: {}",
                        client.base_url, client.model
                    );
                    return Ok(client);
                }
                Err(e) => {
                    warn!(
                        "Ollama connectivity check failed (attempt {}): {}",
                        i + 1,
                        e
                    );
                    last_err = Some(e);
                }
            }
        }

        Err(last_err.unwrap_or(LlmError::EmptyResponse))
    }

    async fn check_circuit_breaker(&self) -> Result<(), LlmError> {
        if self.inner.consecutive_failures.load(Ordering::Relaxed) >= 5 {
            let last = self.inner.last_failure.lock().await;
            if let Some(instant) = *last {
                if instant.elapsed() < Duration::from_secs(30) {
                    return Err(LlmError::CircuitOpen);
                }
            }
        }
        Ok(())
    }

    fn record_success(&self) {
        self.inner.consecutive_failures.store(0, Ordering::Relaxed);
    }

    async fn record_failure(&self) {
        self.inner
            .consecutive_failures
            .fetch_add(1, Ordering::Relaxed);
        let mut last = self.inner.last_failure.lock().await;
        *last = Some(Instant::now());
    }

    /// Send a chat completion request.
    ///
    /// # Errors
    /// Returns `LlmError` on HTTP failure, empty response, or JSON parse issues.
    pub async fn chat(
        &self,
        system: &str,
        user: &str,
        json_mode: bool,
        timeout: Duration,
    ) -> Result<String, LlmError> {
        self.check_circuit_breaker().await?;

        let url = format!("{}/api/chat", self.base_url);

        let mut body = json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user}
            ],
            "stream": false,
            "options": {
                "temperature": if json_mode { 0.0 } else { 0.3 }
            }
        });

        if json_mode {
            body["format"] = json!("json");
        }

        let attempt = || async {
            self.http
                .post(&url)
                .json(&body)
                .timeout(timeout)
                .send()
                .await?
                .error_for_status()
        };

        // Retry logic: 2 retries with backoff
        let mut last_error = None;
        for i in 0..=2 {
            if i > 0 {
                let delay_ms = (500_u64 * 2_u64.pow(i - 1)).min(8000);
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            }

            match attempt().await {
                Ok(resp) => {
                    let ollama: OllamaResponse = resp.json().await?;
                    let content = ollama
                        .message
                        .map(|m| m.content)
                        .or(ollama.response)
                        .unwrap_or_default();

                    if content.is_empty() {
                        self.record_failure().await;
                        return Err(LlmError::EmptyResponse);
                    }
                    self.record_success();
                    return Ok(content);
                }
                Err(e) => {
                    let should_retry = e.is_connect()
                        || e.is_timeout()
                        || e.status().map_or(false, |s| s.is_server_error());
                    if i < 2 && should_retry {
                        warn!("LLM attempt {i} failed, retrying: {e}");
                        last_error = Some(e);
                        continue;
                    }
                    self.record_failure().await;
                    return Err(LlmError::Http(e));
                }
            }
        }

        Err(LlmError::Http(last_error.unwrap()))
    }

    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Verify the Ollama server is reachable.
    ///
    /// # Errors
    /// Returns `LlmError` on connectivity failure.
    pub async fn ping(&self) -> Result<(), LlmError> {
        let _: TagsResponse = self
            .http
            .get(format!("{}/api/tags", self.base_url))
            .timeout(Duration::from_secs(5))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(())
    }
}

/// Trait abstrayant un backend de chat LLM.
///
/// Permet à `AgentBase` d'accepter aussi bien un `LlmClient` direct
/// qu'un wrapper qui ajoute du tracking (par ex. `TokenJuicedLlm`).
#[async_trait::async_trait]
pub trait ChatBackend: Send + Sync + std::fmt::Debug {
    /// Send a chat completion request.
    async fn chat(
        &self,
        system: &str,
        user: &str,
        json_mode: bool,
        timeout: Duration,
    ) -> Result<String, LlmError>;

    /// Get the model name.
    fn model(&self) -> &str;
}

#[async_trait::async_trait]
impl ChatBackend for LlmClient {
    async fn chat(
        &self,
        system: &str,
        user: &str,
        json_mode: bool,
        timeout: Duration,
    ) -> Result<String, LlmError> {
        Self::chat(self, system, user, json_mode, timeout).await
    }

    fn model(&self) -> &str {
        Self::model(self)
    }
}
