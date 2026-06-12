use std::time::Duration;

use crate::errors::CoreError;

#[async_trait::async_trait]
pub trait Engine: Send + Sync {
    type Input: Send + Sync;
    type Output: Send + Sync;

    fn name(&self) -> &'static str;

    async fn run(&self, input: Self::Input, timeout: Duration) -> Result<Self::Output, CoreError>;
}

/// Pipeline stage enum for tracing and fault tracking.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub enum PipelineStage {
    Scout,
    Vision,
    Cortex,
    Mimic,
    Anticlone,
    Sandbox,
}
