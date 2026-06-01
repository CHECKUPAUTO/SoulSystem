use std::sync::Arc;
use tracing::{debug, info};

use crate::compiler::{CompileEvent, CompilerError, NcpuCompiler};
use crate::pattern::PatternSignature;
use crate::store::{PatternStats, PatternStore, PatternStoreError};

#[derive(Debug, Clone)]
pub struct FeedbackConfig {
    pub compile_threshold: u64,
    pub originality_floor: f32,
    pub success_floor: f32,
}

impl Default for FeedbackConfig {
    fn default() -> Self {
        Self {
            compile_threshold: 5,
            originality_floor: 0.7,
            success_floor: 0.8,
        }
    }
}

pub struct FeedbackLoop<C: NcpuCompiler> {
    store: Arc<PatternStore>,
    compiler: Arc<C>,
    config: FeedbackConfig,
}

#[derive(Debug, thiserror::Error)]
pub enum FeedbackError {
    #[error("store: {0}")]
    Store(#[from] PatternStoreError),
    #[error("compiler: {0}")]
    Compiler(#[from] CompilerError),
}

impl<C: NcpuCompiler> FeedbackLoop<C> {
    pub fn new(store: Arc<PatternStore>, compiler: Arc<C>, config: FeedbackConfig) -> Self {
        Self {
            store,
            compiler,
            config,
        }
    }

    pub async fn observe(
        &self,
        task_text: &str,
        url: Option<&str>,
        sandbox_success: bool,
        originality: f32,
        plan: Option<serde_json::Value>,
    ) -> Result<(), FeedbackError> {
        let sig = PatternSignature::from_task(task_text, url);
        let stats = self.store.record(&sig, sandbox_success, originality)?;

        debug!(
            hash = %stats.hash, seen = stats.seen_count,
            success_rate = stats.success_rate(),
            originality_avg = stats.originality_avg(),
            "pattern observed"
        );

        if Self::should_compile(&stats, &self.config) {
            let event = CompileEvent {
                pattern_hash: stats.hash.clone(),
                canonical: stats.canonical.clone(),
                seen_count: stats.seen_count,
                success_rate: stats.success_rate(),
                originality_avg: stats.originality_avg(),
                recent_plans: plan.map(|p| vec![p]).unwrap_or_default(),
            };
            info!(hash = %stats.hash, "emitting compile event to nCPU");
            self.compiler.emit(&event).await?;
            self.store.mark_compiled(&stats.hash)?;
        }

        Ok(())
    }

    fn should_compile(stats: &PatternStats, config: &FeedbackConfig) -> bool {
        !stats.ncpu_compiled
            && stats.success_count >= config.compile_threshold
            && stats.originality_avg() >= config.originality_floor
            && stats.success_rate() >= config.success_floor
    }
}
