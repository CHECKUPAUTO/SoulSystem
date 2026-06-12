//! LLM — Réflexion avancée via Ollama avec validation schema.
//!
//! Délègue à `soulsystem_common::llm_client::OllamaClient` pour le transport HTTP.

pub use soulsystem_common::llm_client::{ChatMessage, LlmResponse, OllamaClient};

/// Wrapper backward-compatible autour de `OllamaClient`.
#[derive(Debug, Clone)]
pub struct LlmEngine {
    inner: OllamaClient,
}

impl LlmEngine {
    pub fn new(url: &str, model: &str, _client: reqwest::Client) -> Self {
        Self {
            inner: OllamaClient::new(url, model),
        }
    }

    pub fn model_name(&self) -> &str {
        self.inner.model_name()
    }

    pub async fn reflect(
        &self,
        context: &str,
        goal: &str,
        system_state: &str,
        _security_guidance: &str,
    ) -> Option<LlmResponse> {
        self.inner.reflect(context, goal, system_state).await
    }

    pub async fn generate_code(
        &self,
        file_path: &str,
        issue: &str,
        current_code: &str,
    ) -> Option<String> {
        self.inner
            .generate_code(file_path, issue, current_code)
            .await
    }
}
