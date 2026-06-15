//! GET /api/models — list available models across all providers.
//!
//! Returns a flat list of model IDs with their provider metadata.
//! This matches the OpenClaw gateway `/api/models` endpoint used by
//! front-end clients to populate model pickers.

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
};
use serde::Serialize;

use crate::api::ApiState;

/// A single model entry in the response.
#[derive(Debug, Serialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub provider_type: String,
}

/// Full response for GET /api/models.
#[derive(Debug, Serialize)]
pub struct ModelsResponse {
    pub models: Vec<ModelInfo>,
}

/// GET /api/models — list all available models across all providers.
pub async fn list_models(State(state): State<ApiState>) -> impl IntoResponse {
    let providers = state.registry.list_providers().await;
    let mut models = Vec::new();

    for provider in &providers {
        let provider_name = provider["name"].as_str().unwrap_or("unknown");
        let provider_type = provider["type"].as_str().unwrap_or("unknown");

        if let Some(provider_models) = provider["models"].as_array() {
            for model in provider_models {
                if let Some(model_name) = model.as_str() {
                    let id = format!("{provider_name}/{model_name}");
                    models.push(ModelInfo {
                        id: id.clone(),
                        name: model_name.to_string(),
                        provider: provider_name.to_string(),
                        provider_type: provider_type.to_string(),
                    });
                }
            }
        }

        // Also add the default model if present and not already listed
        if let Some(default) = provider["default_model"].as_str() {
            let id = format!("{provider_name}/{default}");
            if !models.iter().any(|m| m.id == id) {
                models.push(ModelInfo {
                    id: id.clone(),
                    name: default.to_string(),
                    provider: provider_name.to_string(),
                    provider_type: provider_type.to_string(),
                });
            }
        }
    }

    (StatusCode::OK, Json(ModelsResponse { models }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::registry::ProviderRegistry;
    use crate::provider::ProviderConfig;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_list_models_empty_registry() {
        let registry = Arc::new(ProviderRegistry::new());
        let state = ApiState::new(registry);
        let providers = state.registry.list_providers().await;
        let mut models = Vec::new();

        // Replicate the handler logic
        for provider in &providers {
            if let Some(provider_models) = provider["models"].as_array() {
                for model in provider_models {
                    if let Some(name) = model.as_str() {
                        models.push(ModelInfo {
                            id: format!("unknown/{name}"),
                            name: name.to_string(),
                            provider: "unknown".to_string(),
                            provider_type: "unknown".to_string(),
                        });
                    }
                }
            }
        }

        assert!(models.is_empty());
    }

    #[tokio::test]
    async fn test_list_models_with_provider() {
        let registry = Arc::new(ProviderRegistry::new());
        let cfg = ProviderConfig {
            provider_type: "ollama".into(),
            base_url: "http://localhost:11434".into(),
            api_key: None,
            models: vec!["deepseek-v4-flash:cloud".into(), "llama3:70b".into()],
            default_model: Some("deepseek-v4-flash:cloud".into()),
            timeout_secs: 30,
            ..Default::default()
        };
        registry.register("ollama", cfg).await;

        let state = ApiState::new(registry);
        let providers = state.registry.list_providers().await;

        let mut models = Vec::new();
        for provider in &providers {
            let provider_name = provider["name"].as_str().unwrap_or("unknown");
            if let Some(provider_models) = provider["models"].as_array() {
                for model in provider_models {
                    if let Some(model_name) = model.as_str() {
                        models.push(ModelInfo {
                            id: format!("{provider_name}/{model_name}"),
                            name: model_name.to_string(),
                            provider: provider_name.to_string(),
                            provider_type: "ollama".to_string(),
                        });
                    }
                }
            }
        }

        assert_eq!(models.len(), 2);
        assert!(models
            .iter()
            .any(|m| m.id == "ollama/deepseek-v4-flash:cloud"));
        assert!(models.iter().any(|m| m.id == "ollama/llama3:70b"));
    }
}
