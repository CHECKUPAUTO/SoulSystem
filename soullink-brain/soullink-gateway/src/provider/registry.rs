//! Provider registry — manages available LLM providers with load balancing,
//! circuit breaker support, and fallback logic.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::provider::ollama::OllamaProvider;
use crate::provider::openai::OpenAIProvider;
use crate::provider::{
    ChatRequest, ChatResponse, CompletionRequest, CompletionResponse, Provider, ProviderConfig,
    ProviderError,
};

/// Wrapper around a provider with circuit breaker state.
struct ProviderEntry {
    provider: Arc<dyn Provider>,
    /// Whether this provider is currently circuit-broken.
    circuit_open: bool,
}

/// Thread-safe provider registry with round-robin load balancing.
pub struct ProviderRegistry {
    providers: RwLock<HashMap<String, ProviderEntry>>,
    /// Ordered list of provider names for round-robin iteration.
    ordered: RwLock<Vec<String>>,
    rr_index: RwLock<usize>,
}

impl ProviderRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            providers: RwLock::new(HashMap::new()),
            ordered: RwLock::new(Vec::new()),
            rr_index: RwLock::new(0),
        }
    }

    /// Build registry from config map, auto-detecting provider type.
    pub async fn from_config(configs: HashMap<String, ProviderConfig>) -> Self {
        let registry = Self::new();
        for (name, cfg) in configs {
            registry.register(&name, cfg).await;
        }
        registry
    }

    /// Register a provider from its config.
    pub async fn register(&self, name: &str, cfg: ProviderConfig) {
        let provider: Arc<dyn Provider> = match cfg.provider_type.as_str() {
            "ollama" => Arc::new(OllamaProvider::new(name.to_string(), cfg)),
            "openai" => Arc::new(OpenAIProvider::new(name.to_string(), cfg)),
            other => {
                warn!(provider_type = %other, name = %name, "unknown provider type, defaulting to ollama");
                let ollama_cfg = ProviderConfig {
                    provider_type: "ollama".into(),
                    ..cfg
                };
                Arc::new(OllamaProvider::new(name.to_string(), ollama_cfg))
            }
        };

        info!(name = %name, "registered provider");

        let mut providers = self.providers.write().await;
        let mut ordered = self.ordered.write().await;

        providers.insert(
            name.to_string(),
            ProviderEntry {
                provider,
                circuit_open: false,
            },
        );
        ordered.push(name.to_string());
    }

    /// Get a specific provider by name.
    pub async fn get(&self, name: &str) -> Option<Arc<dyn Provider>> {
        let providers = self.providers.read().await;
        providers.get(name).map(|entry| entry.provider.clone())
    }

    /// Find a provider by model ID (handles both "provider/model" and plain model names).
    pub async fn resolve(&self, model: Option<&str>) -> Result<Arc<dyn Provider>, ProviderError> {
        let model = model.unwrap_or("");

        // Try exact provider name
        let providers = self.providers.read().await;
        let ordered = self.ordered.read().await;

        if providers.is_empty() {
            return Err(ProviderError::NotFound("no providers configured".into()));
        }

        // If model has "provider/" prefix, look up that specific provider
        if let Some((provider_name, _model_id)) = model.split_once('/') {
            if let Some(entry) = providers.get(provider_name) {
                if !entry.circuit_open {
                    return Ok(entry.provider.clone());
                }
                return Err(ProviderError::CircuitOpen(provider_name.to_string()));
            }
            // Provider name not found — fall through to default
            warn!(provider = %provider_name, "explicit provider not found, using default");
        }

        // Round-robin across all non-circuit-broken providers
        let n = ordered.len();
        let mut idx = self.rr_index.read().await.clone();
        for _ in 0..n {
            let name = &ordered[idx % n];
            idx = (idx + 1) % n;
            if let Some(entry) = providers.get(name) {
                if !entry.circuit_open {
                    let mut rr_idx = self.rr_index.write().await;
                    *rr_idx = idx;
                    return Ok(entry.provider.clone());
                }
            }
        }

        // All providers circuit-broken
        Err(ProviderError::NotFound(
            "all providers circuit-broken".into(),
        ))
    }

    /// Mark a provider's circuit as open (tripped).
    pub async fn trip_circuit(&self, name: &str) {
        let mut providers = self.providers.write().await;
        if let Some(entry) = providers.get_mut(name) {
            entry.circuit_open = true;
            warn!(name = %name, "circuit breaker tripped");
        }
    }

    /// Reset a provider's circuit breaker.
    pub async fn reset_circuit(&self, name: &str) {
        let mut providers = self.providers.write().await;
        if let Some(entry) = providers.get_mut(name) {
            entry.circuit_open = false;
            info!(name = %name, "circuit breaker reset");
        }
    }

    /// List all registered providers with their status.
    pub async fn list_providers(&self) -> Vec<serde_json::Value> {
        let providers = self.providers.read().await;
        let ordered = self.ordered.read().await;

        ordered
            .iter()
            .filter_map(|name| {
                providers.get(name).map(|entry| {
                    let cfg = entry.provider.config();
                    serde_json::json!({
                        "name": name,
                        "type": cfg.provider_type,
                        "base_url": cfg.base_url,
                        "models": cfg.models,
                        "default_model": cfg.default_model,
                        "circuit_open": entry.circuit_open,
                    })
                })
            })
            .collect()
    }

    /// Number of registered providers.
    pub async fn len(&self) -> usize {
        self.providers.read().await.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_empty_registry() {
        let registry = ProviderRegistry::new();
        assert_eq!(registry.len().await, 0);
        let result = registry.resolve(Some("test")).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_register_and_get() {
        let registry = ProviderRegistry::new();
        let cfg = ProviderConfig {
            provider_type: "ollama".into(),
            base_url: "http://localhost:11434".into(),
            api_key: None,
            models: vec!["deepseek-v4-flash:cloud".into()],
            default_model: Some("deepseek-v4-flash:cloud".into()),
            timeout_secs: 30,
        };
        registry.register("ollama", cfg).await;
        assert_eq!(registry.len().await, 1);

        let provider = registry.get("ollama").await;
        assert!(provider.is_some());
        assert_eq!(provider.unwrap().name(), "ollama");
    }

    #[tokio::test]
    async fn test_resolve_by_explicit_name() {
        let registry = ProviderRegistry::new();
        let cfg = ProviderConfig {
            provider_type: "ollama".into(),
            base_url: "http://localhost:11434".into(),
            api_key: None,
            models: vec![],
            default_model: None,
            timeout_secs: 30,
        };
        registry.register("ollama", cfg).await;
        let provider = registry
            .resolve(Some("ollama/deepseek-v4-flash:cloud"))
            .await;
        assert!(provider.is_ok());
        assert_eq!(provider.unwrap().name(), "ollama");
    }

    #[tokio::test]
    async fn test_circuit_breaker() {
        let registry = ProviderRegistry::new();
        let cfg = ProviderConfig {
            provider_type: "ollama".into(),
            base_url: "http://localhost:11434".into(),
            api_key: None,
            models: vec![],
            default_model: None,
            timeout_secs: 30,
        };
        registry.register("ollama", cfg).await;

        registry.trip_circuit("ollama").await;
        let result = registry.resolve(Some("ollama/test")).await;
        assert!(result.is_err());
        assert!(matches!(result, Err(ProviderError::CircuitOpen(_))));

        registry.reset_circuit("ollama").await;
        let result = registry.resolve(Some("ollama/test")).await;
        assert!(result.is_ok());
    }
}
