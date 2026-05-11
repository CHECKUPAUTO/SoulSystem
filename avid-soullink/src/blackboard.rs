use std::time::Duration;

use async_trait::async_trait;
use futures::stream::{Stream, StreamExt};
use tracing::{debug, warn};

use crate::intent::{Intent, IntentResult};

#[derive(Debug, thiserror::Error)]
pub enum BlackboardError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("serde error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("subscription closed")]
    Closed,
}

#[async_trait]
pub trait Blackboard: Send + Sync {
    async fn publish_result(&self, result: &IntentResult) -> Result<(), BlackboardError>;
    async fn subscribe_intents(
        &self,
        topic: &str,
    ) -> Result<
        std::pin::Pin<Box<dyn Stream<Item = Result<Intent, BlackboardError>> + Send>>,
        BlackboardError,
    >;
}

/// Implémentation HTTP/SSE pour le Blackboard SoulLink.
/// Endpoint réel: `:9074` (pas `:7460` comme supposé dans le doc).
pub struct HttpBlackboard {
    base_url: String,
    http: reqwest::Client,
}

impl HttpBlackboard {
    #[must_use]
    pub fn new(base_url: impl Into<String>) -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(300))
            .build()
            .expect("reqwest client build");
        Self { base_url: base_url.into(), http }
    }
}

#[async_trait]
impl Blackboard for HttpBlackboard {
    async fn publish_result(&self, result: &IntentResult) -> Result<(), BlackboardError> {
        let url = format!("{}/publish", self.base_url);
        let resp = self.http.post(&url).json(result).send().await?;
        if !resp.status().is_success() {
            warn!(status = ?resp.status(), "blackboard publish non-2xx");
        }
        Ok(())
    }

    async fn subscribe_intents(
        &self,
        topic: &str,
    ) -> Result<
        std::pin::Pin<Box<dyn Stream<Item = Result<Intent, BlackboardError>> + Send>>,
        BlackboardError,
    > {
        let url = format!("{}/subscribe?topic={topic}", self.base_url);
        let resp = self.http
            .get(&url)
            .header(reqwest::header::ACCEPT, "text/event-stream")
            .send()
            .await?;

        let stream = resp.bytes_stream().map(|chunk_res| {
            chunk_res.map_err(BlackboardError::Http).and_then(|chunk| {
                let s = String::from_utf8_lossy(&chunk);
                let line = s.trim_start_matches("data: ").trim();
                if line.is_empty() {
                    return Err(BlackboardError::Closed);
                }
                let intent: Intent = serde_json::from_str(line)?;
                debug!(intent_id = %intent.id, topic = %intent.topic, "intent received");
                Ok(intent)
            })
        });
        Ok(Box::pin(stream))
    }
}
