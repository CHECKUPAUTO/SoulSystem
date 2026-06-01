use async_trait::async_trait;
use tracing::debug;

use crate::intent::IntentResult;

#[derive(Debug, thiserror::Error)]
pub enum ArchiveError {
    #[error("memori error: {0}")]
    Memori(String),
    #[error("serde error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
}

#[async_trait]
pub trait Archive: Send + Sync {
    async fn store(&self, key: &str, result: &IntentResult) -> Result<(), ArchiveError>;
}

/// Implémentation réelle via l'API REST de Memori (`:9075`).
/// Routes: POST /api/archive, POST /api/write_fact
pub struct MemoriArchive {
    memori_url: String,
    namespace: String,
    http: reqwest::Client,
}

impl MemoriArchive {
    #[must_use]
    pub fn new(memori_url: impl Into<String>, namespace: impl Into<String>) -> Self {
        Self {
            memori_url: memori_url.into(),
            namespace: namespace.into(),
            http: reqwest::Client::new(),
        }
    }

    /// Constructeur simplifié pour compatibilité avec le doc original
    #[must_use]
    pub fn with_namespace(namespace: impl Into<String>) -> Self {
        Self::new("http://127.0.0.1:9075", namespace)
    }
}

#[async_trait]
impl Archive for MemoriArchive {
    async fn store(&self, key: &str, result: &IntentResult) -> Result<(), ArchiveError> {
        let full_key = format!("{}:{key}", self.namespace);
        let value = serde_json::to_string(result)?;

        // Appel réel à l'API Memori /api/archive
        let resp = self
            .http
            .post(format!("{}/api/archive", self.memori_url))
            .json(&serde_json::json!({
                "key": full_key,
                "value": value,
                "timestamp": result.finished_at,
            }))
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(ArchiveError::Memori(format!("{status}: {body}")));
        }

        debug!(key = %full_key, bytes = value.len(), "memori store OK");
        Ok(())
    }
}
