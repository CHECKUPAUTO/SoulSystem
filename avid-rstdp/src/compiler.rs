use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::warn;

#[derive(Debug, thiserror::Error)]
pub enum CompilerError {
    #[error("http: {0}")]
    Http(#[from] reqwest::Error),
    #[error("compiler refused: {0}")]
    Refused(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileEvent {
    pub pattern_hash: String,
    pub canonical: String,
    pub seen_count: u64,
    pub success_rate: f32,
    pub originality_avg: f32,
    pub recent_plans: Vec<serde_json::Value>,
}

#[async_trait]
pub trait NcpuCompiler: Send + Sync {
    async fn emit(&self, event: &CompileEvent) -> Result<(), CompilerError>;
}

/// Implémentation HTTP vers le instinct-daemon.
/// Note: le daemon n'existe pas encore (port :7451 libre).
/// Ce client est prêt pour quand il sera déployé.
pub struct HttpNcpuCompiler {
    base_url: String,
    http: reqwest::Client,
}

impl HttpNcpuCompiler {
    #[must_use]
    pub fn new(base_url: impl Into<String>) -> Self {
        Self { base_url: base_url.into(), http: reqwest::Client::new() }
    }
}

#[async_trait]
impl NcpuCompiler for HttpNcpuCompiler {
    async fn emit(&self, event: &CompileEvent) -> Result<(), CompilerError> {
        let url = format!("{}/patterns/compile", self.base_url);
        let resp = self.http.post(&url).json(event).send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            warn!(%status, %body, "ncpu compiler returned non-2xx");
            return Err(CompilerError::Refused(format!("{status}: {body}")));
        }
        Ok(())
    }
}
