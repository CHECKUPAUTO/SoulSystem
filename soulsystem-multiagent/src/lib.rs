pub mod agent;
pub mod cache;
pub mod orchestrator;

use anyhow::Result;
use tracing::info;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MultiAgentConfig {
    pub queue_capacity: usize,
    pub cache_ttl_secs: u64,
}

impl Default for MultiAgentConfig {
    fn default() -> Self {
        Self {
            queue_capacity: 1000,
            cache_ttl_secs: 3600,
        }
    }
}

pub async fn run(config: MultiAgentConfig) -> Result<()> {
    info!("Starting multi-agent system with config: {:?}", config);
    let orch = orchestrator::Orchestrator::new(config).await?;
    orch.serve().await
}
