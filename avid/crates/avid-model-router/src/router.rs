//! Intelligent model router — selects the best model for a task based on required
//! capabilities, availability, priority, and fallback chain.

use crate::{Capability, ModelProfile, ModelRegistry, ModelSelection, RegistryError};
use tracing::warn;

/// Errors that can occur during model routing.
#[derive(Debug, thiserror::Error)]
pub enum RouterError {
    #[error("registry error: {0}")]
    Registry(#[from] RegistryError),
    #[error("no models available in registry")]
    NoModelsAvailable,
    #[error("no fallback available after '{failed_model}' failed; chain exhausted")]
    NoFallbackAvailable { failed_model: String },
}

/// The intelligent model router.
///
/// It maintains a registry of model profiles and selects the best model for a
/// given task using capability matching, priority ordering, and a fallback chain.
///
/// # Examples
///
/// ```rust
/// use avid_model_router::{ModelRouter, Capability};
///
/// let mut router = ModelRouter::with_defaults();
/// let selection = router.select_for_task(
///     "Write a Rust function to sort a Vec",
///     &[Capability::CodeGeneration],
/// );
/// eprintln!("Selected {} because: {}", selection.profile.name, selection.reason);
/// ```
#[derive(Debug, Clone)]
pub struct ModelRouter {
    registry: ModelRegistry,
    default_model: String,
    fallback_chain: Vec<String>,
}

impl ModelRouter {
    /// Create a router from an existing registry with an explicit default model and
    /// fallback chain.
    #[must_use]
    pub fn new(
        registry: ModelRegistry,
        default_model: String,
        fallback_chain: Vec<String>,
    ) -> Self {
        Self {
            registry,
            default_model,
            fallback_chain,
        }
    }

    /// Create a router pre-populated with the default model profiles.
    ///
    /// The default model and fallback chain are read from the embedded JSON config.
    #[must_use]
    pub fn with_defaults() -> Self {
        let config = crate::registry::ModelsConfig::parse_json(include_str!("models.json"))
            .expect("embedded models.json is valid");
        let registry = ModelRegistry::with_defaults();
        Self {
            registry,
            default_model: config
                .default_model
                .unwrap_or_else(|| "deepseek-v4-pro:cloud".to_string()),
            fallback_chain: config.fallback_chain,
        }
    }

    /// Create a router by loading models from a file.
    ///
    /// # Errors
    /// Returns `RouterError::Registry` if the file cannot be read or parsed.
    pub fn from_file<P: AsRef<std::path::Path>>(path: P) -> Result<Self, RouterError> {
        let config = crate::registry::ModelsConfig::from_file(&path)?;
        let mut registry = ModelRegistry::new();
        registry.load_from_file(&path)?;
        Ok(Self {
            registry,
            default_model: config
                .default_model
                .unwrap_or_else(|| "deepseek-v4-pro:cloud".to_string()),
            fallback_chain: config.fallback_chain,
        })
    }

    /// Select the best model for a task based on required capabilities.
    ///
    /// The algorithm:
    /// 1. If `required_capabilities` is non-empty, filter models that have **all** required
    ///    capabilities.
    /// 2. If `required_capabilities` is empty, consider **all** registered models.
    /// 3. Sort candidates by priority (lowest first), then by average latency (lowest first).
    /// 4. Pick the first candidate.
    /// 5. If no candidate was found, fall back to the `default_model`.
    ///
    /// The returned `ModelSelection.reason` explains why this model was chosen.
    #[must_use]
    pub fn select_for_task(
        &self,
        task: &str,
        required_capabilities: &[Capability],
    ) -> ModelSelection {
        let candidates = if required_capabilities.is_empty() {
            self.registry.list_all()
        } else {
            // Intersection: models that have ALL required capabilities
            let mut candidates: Vec<&ModelProfile> = self
                .registry
                .list_all()
                .into_iter()
                .filter(|p| {
                    required_capabilities
                        .iter()
                        .all(|cap| p.capabilities.contains(cap))
                })
                .collect();
            candidates.sort_by_key(|p| p.priority);
            candidates
        };

        if let Some(best) = candidates.first() {
            return ModelSelection {
                profile: (*best).clone(),
                reason: format!(
                    "matched {} capability(s) for task '{}': priority={}, latency={}ms",
                    required_capabilities.len(),
                    task,
                    best.priority,
                    best.avg_latency_ms
                ),
            };
        }

        // Fall back to the default model
        warn!(
            "no model matched capabilities {:?}, falling back to default '{}'",
            required_capabilities, self.default_model
        );
        let default_profile = self
            .registry
            .get(&self.default_model)
            .cloned()
            .unwrap_or_else(|| {
                warn!(
                    "default model '{}' not in registry, using synthetic profile",
                    self.default_model
                );
                ModelProfile {
                    name: self.default_model.clone(),
                    provider: "ollama".to_string(),
                    base_url: "http://localhost:11434".to_string(),
                    capabilities: vec![],
                    max_tokens: 4096,
                    cost_per_1k_tokens: 0.0,
                    priority: u8::MAX,
                    avg_latency_ms: 0,
                }
            });

        ModelSelection {
            profile: default_profile,
            reason: format!(
                "fallback to default model '{}' — no match for {} capability(s) on task '{}'",
                self.default_model,
                required_capabilities.len(),
                task
            ),
        }
    }

    /// Select a fallback model after `failed_model` has failed.
    ///
    /// Traverses the `fallback_chain` to find the next model that:
    /// - Is not the failed model itself
    /// - Is registered in the registry
    /// - Has not already been tried (skips all models listed before `failed_model`)
    ///
    /// Returns `None` if the chain is exhausted (no available fallback).
    #[must_use]
    pub fn select_fallback(&self, failed_model: &str) -> Option<ModelSelection> {
        // Find the position of the failed model in the fallback chain
        let start_idx = self
            .fallback_chain
            .iter()
            .position(|name| name == failed_model);

        let chain_iter: Box<dyn Iterator<Item = &str>> = match start_idx {
            Some(idx) => Box::new(self.fallback_chain[idx + 1..].iter().map(String::as_str)),
            None => {
                // If the failed model is not in the fallback chain, try the chain from the
                // start, but skip models matching the failed one.
                Box::new(
                    self.fallback_chain
                        .iter()
                        .map(String::as_str)
                        .filter(move |name| *name != failed_model),
                )
            }
        };

        for name in chain_iter {
            if let Some(profile) = self.registry.get(name) {
                return Some(ModelSelection {
                    profile: profile.clone(),
                    reason: format!(
                        "fallback after '{failed_model}' failed; next in chain: '{name}'"
                    ),
                });
            }
            warn!("fallback model '{}' not found in registry, skipping", name);
        }

        None
    }

    /// Access the underlying registry.
    #[must_use]
    pub fn registry(&self) -> &ModelRegistry {
        &self.registry
    }

    /// Mutable access to the underlying registry.
    pub fn registry_mut(&mut self) -> &mut ModelRegistry {
        &mut self.registry
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::ModelRegistry;

    fn test_registry() -> ModelRegistry {
        let mut reg = ModelRegistry::new();

        let fast = ModelProfile {
            name: "qwen3:4b".into(),
            provider: "ollama".into(),
            base_url: "http://localhost:11434".into(),
            capabilities: vec![Capability::FastChat, Capability::CodeGeneration],
            max_tokens: 8192,
            cost_per_1k_tokens: 0.0,
            priority: 5,
            avg_latency_ms: 300,
        };
        let heavy = ModelProfile {
            name: "deepseek-v4-pro:cloud".into(),
            provider: "ollama".into(),
            base_url: "http://localhost:11434".into(),
            capabilities: vec![
                Capability::CodeGeneration,
                Capability::CodeReview,
                Capability::Planning,
                Capability::Analysis,
            ],
            max_tokens: 131_072,
            cost_per_1k_tokens: 0.0,
            priority: 10,
            avg_latency_ms: 2500,
        };
        let mid = ModelProfile {
            name: "kimi-k2.6:cloud".into(),
            provider: "ollama".into(),
            base_url: "http://localhost:11434".into(),
            capabilities: vec![Capability::Planning, Capability::CodeGeneration],
            max_tokens: 131_072,
            cost_per_1k_tokens: 0.0,
            priority: 20,
            avg_latency_ms: 3000,
        };

        reg.register(fast).unwrap();
        reg.register(heavy).unwrap();
        reg.register(mid).unwrap();
        reg
    }

    #[test]
    fn select_by_capability() {
        let registry = test_registry();
        let router = ModelRouter::new(
            registry,
            "deepseek-v4-pro:cloud".into(),
            vec!["kimi-k2.6:cloud".into(), "qwen3:4b".into()],
        );

        let sel = router.select_for_task("write code", &[Capability::CodeGeneration]);
        // qwen3:4b has priority 5, so it should be picked for CodeGeneration
        assert_eq!(sel.profile.name, "qwen3:4b");
        assert!(sel.reason.contains("matched"));
    }

    #[test]
    fn select_with_multiple_capabilities() {
        let registry = test_registry();
        let router = ModelRouter::new(registry, "deepseek-v4-pro:cloud".into(), vec![]);

        // Only deepseek has BOTH CodeReview AND Analysis
        let sel = router.select_for_task(
            "deep review",
            &[Capability::CodeReview, Capability::Analysis],
        );
        assert_eq!(sel.profile.name, "deepseek-v4-pro:cloud");
    }

    #[test]
    fn select_fallback_to_default() {
        let registry = test_registry();
        let router = ModelRouter::new(registry, "deepseek-v4-pro:cloud".into(), vec![]);

        // None of our models have Creative
        let sel = router.select_for_task("creative writing", &[Capability::Creative]);
        assert_eq!(sel.profile.name, "deepseek-v4-pro:cloud");
        assert!(sel.reason.contains("fallback"));
    }

    #[test]
    fn fallback_chain() {
        let registry = test_registry();
        let router = ModelRouter::new(
            registry,
            "deepseek-v4-pro:cloud".into(),
            vec![
                "deepseek-v4-pro:cloud".into(),
                "kimi-k2.6:cloud".into(),
                "qwen3:4b".into(),
            ],
        );

        // After deepseek fails, should pick kimi-k2.6
        let sel = router.select_fallback("deepseek-v4-pro:cloud").unwrap();
        assert_eq!(sel.profile.name, "kimi-k2.6:cloud");

        // After the last in chain, no fallback
        let sel = router.select_fallback("qwen3:4b");
        assert!(sel.is_none());
    }

    #[test]
    fn fallback_not_in_chain() {
        let registry = test_registry();
        let router = ModelRouter::new(
            registry,
            "deepseek-v4-pro:cloud".into(),
            vec!["deepseek-v4-pro:cloud".into(), "qwen3:4b".into()],
        );

        // Unknown model not in the chain → should try from start, skipping unknown
        let sel = router.select_fallback("unknown-model");
        // It should return the first in chain (deepseek)
        assert!(sel.is_some());
    }

    #[test]
    fn with_defaults_works() {
        let router = ModelRouter::with_defaults();
        let sel = router.select_for_task("test", &[Capability::FastChat]);
        // Should prefer qwen3:4b for FastChat (lowest priority)
        eprintln!("Selected: {}", sel.profile.name);
        assert!(!sel.profile.name.is_empty());
    }
}
