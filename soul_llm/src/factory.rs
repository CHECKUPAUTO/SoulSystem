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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_provider_by_name_valid() {
        let config = LlmConfig::default();
        // Ollama n'a pas besoin d'auth token
        let result = create_provider_by_name("ollama", &config);
        assert!(result.is_ok());
    }

    #[test]
    fn create_provider_by_name_invalid() {
        let config = LlmConfig::default();
        let result = create_provider_by_name("nonexistent_provider", &config);
        assert!(result.is_err());
    }

    #[test]
    fn create_provider_by_name_case_insensitive() {
        let config = LlmConfig::default();
        assert!(create_provider_by_name("OLLAMA", &config).is_ok());
    }
}
