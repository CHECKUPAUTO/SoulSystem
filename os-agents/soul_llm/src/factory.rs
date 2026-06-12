use crate::error::Result;
use crate::provider::LlmProvider;
use crate::types::{LlmConfig, ProviderKind};
use std::sync::Arc;

/// Crée un provider à partir de la configuration.
pub fn create_provider(config: &LlmConfig) -> Result<Arc<dyn LlmProvider>> {
    match config.provider {
        ProviderKind::Ollama => Ok(Arc::new(crate::providers::ollama::OllamaProvider::new(
            config,
        ))),
        ProviderKind::OpenAI => Ok(Arc::new(crate::providers::openai::OpenAIProvider::new(
            config,
        )?)),
        ProviderKind::Anthropic => Ok(Arc::new(
            crate::providers::anthropic::AnthropicProvider::new(config)?,
        )),
    }
}

/// Crée un provider par son nom (pour le CLI).
pub fn create_provider_by_name(name: &str, config: &LlmConfig) -> Result<Arc<dyn LlmProvider>> {
    let mut cfg = config.clone();
    cfg.provider = name.parse().map_err(|e: String| {
        crate::error::LlmError::UnknownProvider(e)
    })?;
    create_provider(&cfg)
}
