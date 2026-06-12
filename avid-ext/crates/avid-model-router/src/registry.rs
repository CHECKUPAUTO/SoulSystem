//! Model registry — stores available model profiles and provides lookup by capability.
//!
//! Models are loaded from a JSON config file and can be registered or unregistered
//! at runtime. The default models file is embedded at compile time.

use crate::{Capability, ModelProfile};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use thiserror::Error;

/// Errors that can occur during registry operations.
#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("failed to read models file: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse models JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("model '{0}' is already registered")]
    Duplicate(String),
    #[error("model '{0}' not found")]
    NotFound(String),
}

/// Deserialization helper for the JSON models file.
#[derive(Debug, Deserialize)]
struct ModelsFile {
    models: Vec<ModelProfile>,
    #[serde(default)]
    default_model: Option<String>,
    #[serde(default)]
    fallback_chain: Vec<String>,
}

/// Default models configuration embedded at compile time.
const DEFAULT_MODELS_JSON: &str = include_str!("models.json");

/// A registry of model profiles, keyed by model name.
///
/// # Examples
///
/// ```rust
/// use avid_model_router::{ModelRegistry, Capability};
///
/// let mut registry = ModelRegistry::new();
/// let code_gen_models = registry.find_by_capability(&Capability::CodeGeneration);
/// for profile in code_gen_models {
///     println!("{} (priority: {})", profile.name, profile.priority);
/// }
/// ```
#[derive(Debug, Clone, Default)]
pub struct ModelRegistry {
    models: HashMap<String, ModelProfile>,
}

impl ModelRegistry {
    /// Create an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            models: HashMap::new(),
        }
    }

    /// Create a registry pre-populated with the default models from the embedded JSON config.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use avid_model_router::ModelRegistry;
    ///
    /// let registry = ModelRegistry::with_defaults();
    /// assert!(!registry.list_all().is_empty());
    /// ```
    #[must_use]
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();
        registry
            .load_from_json_str(DEFAULT_MODELS_JSON)
            .expect("embedded models.json is valid");
        registry
    }

    /// Load models from a JSON string, registering all models defined in it.
    ///
    /// # Errors
    /// Returns `RegistryError::Json` if the JSON is malformed.
    /// Returns `RegistryError::Duplicate` if a model name already exists.
    pub fn load_from_json_str(&mut self, json: &str) -> Result<(), RegistryError> {
        let file: ModelsFile = serde_json::from_str(json)?;
        for model in file.models {
            if self.models.contains_key(&model.name) {
                return Err(RegistryError::Duplicate(model.name));
            }
            self.models.insert(model.name.clone(), model);
        }
        Ok(())
    }

    /// Load models from a JSON file at the given path.
    ///
    /// # Errors
    /// Returns `RegistryError::Io` if the file cannot be read.
    /// Returns `RegistryError::Json` if the JSON is malformed.
    pub fn load_from_file<P: AsRef<Path>>(&mut self, path: P) -> Result<(), RegistryError> {
        let content = std::fs::read_to_string(path)?;
        self.load_from_json_str(&content)
    }

    /// Register a single model profile.
    ///
    /// # Errors
    /// Returns `RegistryError::Duplicate` if a model with the same name already exists.
    pub fn register(&mut self, profile: ModelProfile) -> Result<(), RegistryError> {
        if self.models.contains_key(&profile.name) {
            return Err(RegistryError::Duplicate(profile.name));
        }
        self.models.insert(profile.name.clone(), profile);
        Ok(())
    }

    /// Unregister a model by name.
    ///
    /// # Errors
    /// Returns `RegistryError::NotFound` if the model name is not registered.
    pub fn unregister(&mut self, name: &str) -> Result<(), RegistryError> {
        if self.models.remove(name).is_none() {
            return Err(RegistryError::NotFound(name.to_string()));
        }
        Ok(())
    }

    /// Find all models that have a specific capability, sorted by priority (lowest first).
    #[must_use]
    pub fn find_by_capability(&self, cap: &Capability) -> Vec<&ModelProfile> {
        let mut results: Vec<&ModelProfile> = self
            .models
            .values()
            .filter(|p| p.capabilities.contains(cap))
            .collect();
        results.sort_by_key(|p| p.priority);
        results
    }

    /// List all registered models, sorted by priority.
    #[must_use]
    pub fn list_all(&self) -> Vec<&ModelProfile> {
        let mut results: Vec<&ModelProfile> = self.models.values().collect();
        results.sort_by_key(|p| p.priority);
        results
    }

    /// Get a single model profile by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&ModelProfile> {
        self.models.get(name)
    }

    /// Return the number of registered models.
    #[must_use]
    pub fn len(&self) -> usize {
        self.models.len()
    }

    /// Check if the registry is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.models.is_empty()
    }
}

/// Parsed configuration from a models file.
#[derive(Debug, Clone)]
pub struct ModelsConfig {
    pub models: Vec<ModelProfile>,
    pub default_model: Option<String>,
    pub fallback_chain: Vec<String>,
}

impl ModelsConfig {
    /// Parse a models JSON file and return the full configuration.
    ///
    /// # Errors
    /// Returns `RegistryError::Io` if the file cannot be read.
    /// Returns `RegistryError::Json` if the JSON is malformed.
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, RegistryError> {
        let content = std::fs::read_to_string(path)?;
        Self::parse_json(&content)
    }

    /// Parse a models JSON string and return the full configuration.
    ///
    /// # Errors
    /// Returns `RegistryError::Json` if the JSON is malformed.
    pub fn parse_json(json: &str) -> Result<Self, RegistryError> {
        let file: ModelsFile = serde_json::from_str(json)?;
        Ok(Self {
            models: file.models,
            default_model: file.default_model,
            fallback_chain: file.fallback_chain,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_registry() {
        let reg = ModelRegistry::new();
        assert!(reg.is_empty());
        assert_eq!(reg.len(), 0);
    }

    #[test]
    fn register_and_find() {
        let mut reg = ModelRegistry::new();
        let profile = ModelProfile {
            name: "test-model".into(),
            provider: "ollama".into(),
            base_url: "http://localhost:11434".into(),
            capabilities: vec![Capability::FastChat, Capability::CodeGeneration],
            max_tokens: 4096,
            cost_per_1k_tokens: 0.0,
            priority: 10,
            avg_latency_ms: 500,
        };
        reg.register(profile).unwrap();
        assert_eq!(reg.len(), 1);
        assert!(reg.get("test-model").is_some());

        let fast_chat = reg.find_by_capability(&Capability::FastChat);
        assert_eq!(fast_chat.len(), 1);
        assert_eq!(fast_chat[0].name, "test-model");
    }

    #[test]
    fn register_duplicate() {
        let mut reg = ModelRegistry::new();
        let profile = ModelProfile {
            name: "dup".into(),
            provider: "ollama".into(),
            base_url: String::new(),
            capabilities: vec![],
            max_tokens: 1024,
            cost_per_1k_tokens: 0.0,
            priority: 0,
            avg_latency_ms: 0,
        };
        reg.register(profile.clone()).unwrap();
        let err = reg.register(profile).unwrap_err();
        assert!(matches!(err, RegistryError::Duplicate(_)));
    }

    #[test]
    fn unregister() {
        let mut reg = ModelRegistry::new();
        let profile = ModelProfile {
            name: "rm-me".into(),
            provider: "ollama".into(),
            base_url: String::new(),
            capabilities: vec![],
            max_tokens: 1024,
            cost_per_1k_tokens: 0.0,
            priority: 0,
            avg_latency_ms: 0,
        };
        reg.register(profile).unwrap();
        reg.unregister("rm-me").unwrap();
        assert!(reg.get("rm-me").is_none());
    }

    #[test]
    fn unregister_not_found() {
        let mut reg = ModelRegistry::new();
        let err = reg.unregister("ghost").unwrap_err();
        assert!(matches!(err, RegistryError::NotFound(_)));
    }

    #[test]
    fn defaults_loaded() {
        let reg = ModelRegistry::with_defaults();
        assert!(!reg.is_empty());
        assert!(reg.get("deepseek-v4-pro:cloud").is_some());
        assert!(reg.get("qwen3:4b").is_some());
    }

    #[test]
    fn find_by_capability_sorted_by_priority() {
        let reg = ModelRegistry::with_defaults();
        let fast = reg.find_by_capability(&Capability::FastChat);
        // qwen3:4b (priority 5) should come before gemma4 (priority 15)
        if fast.len() >= 2 {
            assert!(fast[0].priority <= fast[1].priority);
        }
    }

    #[test]
    fn parse_models_config() {
        let config = ModelsConfig::parse_json(DEFAULT_MODELS_JSON).unwrap();
        assert!(!config.models.is_empty());
        assert_eq!(
            config.default_model.as_deref(),
            Some("deepseek-v4-pro:cloud")
        );
        assert_eq!(config.fallback_chain.len(), 3);
    }
}
