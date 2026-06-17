use crate::budget::LlmBudget;
use crate::error::Result;
use crate::factory::create_provider;
use crate::provider::LlmProvider;
use crate::types::{GenerateRequest, LlmConfig, ModelInfo, StreamChunk};
use futures::stream::BoxStream;
use std::sync::Arc;

/// Client LLM unifié — wrapper autour de `dyn LlmProvider` + budget.
///
/// Fournit une API backward-compatible avec l'ancien `OllamaClient`
/// tout en supportant n'importe quel provider.
#[derive(Clone)]
pub struct LlmClient {
    provider: Arc<dyn LlmProvider>,
    budget: Arc<LlmBudget>,
    config: LlmConfig,
}

impl LlmClient {
    /// Crée un client à partir de la configuration.
    pub fn new(config: LlmConfig) -> Result<Self> {
        let provider = create_provider(&config)?;
        let budget = Arc::new(LlmBudget::new(config.clone()));
        Ok(Self {
            provider,
            budget,
            config,
        })
    }

    /// Crée un client avec un provider pré-construit.
    pub fn with_provider(provider: Arc<dyn LlmProvider>, config: LlmConfig) -> Self {
        let budget = Arc::new(LlmBudget::new(config.clone()));
        Self {
            provider,
            budget,
            config,
        }
    }

    /// Accès au provider sous-jacent.
    pub fn provider(&self) -> &dyn LlmProvider {
        self.provider.as_ref()
    }

    /// Accès au budget.
    pub fn budget(&self) -> &LlmBudget {
        &self.budget
    }

    /// Accès à la config.
    pub fn config(&self) -> &LlmConfig {
        &self.config
    }

    /// Vérifie que le provider est joignable.
    pub async fn is_alive(&self) -> bool {
        self.provider.health_check().await
    }

    /// Liste les modèles disponibles.
    pub async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        self.provider.list_models().await
    }

    /// Génère une réponse (batch).
    pub async fn generate(&self, prompt: &str) -> Result<crate::types::GenerateResult> {
        self.generate_with_goal(prompt, "default").await
    }

    /// Génère avec suivi de budget pour un goal spécifique.
    pub async fn generate_with_goal(
        &self,
        prompt: &str,
        goal_id: &str,
    ) -> Result<crate::types::GenerateResult> {
        let estimated = LlmBudget::estimate_tokens(prompt, self.config.max_tokens);
        self.budget.check_budget(goal_id, estimated)?;

        let req = GenerateRequest::new(prompt)
            .with_model(self.config.model.clone())
            .with_temperature(self.config.temperature)
            .with_max_tokens(self.config.max_tokens);

        let result = self.provider.generate(&req).await?;

        self.budget.record_usage(goal_id, &result.usage);

        Ok(result)
    }

    /// Génère une réponse et retourne uniquement le texte (compatibilité).
    pub async fn generate_text(&self, prompt: &str) -> Result<String> {
        Ok(self.generate(prompt).await?.text)
    }

    /// Génère avec suivi de budget et retourne uniquement le texte.
    pub async fn generate_text_with_goal(&self, prompt: &str, goal_id: &str) -> Result<String> {
        Ok(self.generate_with_goal(prompt, goal_id).await?.text)
    }

    /// Génération streaming.
    pub async fn generate_stream(
        &self,
        request: GenerateRequest,
    ) -> Result<BoxStream<'static, Result<StreamChunk>>> {
        self.provider.generate_stream(request).await
    }

    /// Génération streaming (raccourci).
    pub async fn stream(&self, prompt: &str) -> Result<BoxStream<'static, Result<StreamChunk>>> {
        let req = GenerateRequest::new(prompt)
            .with_model(self.config.model.clone())
            .with_temperature(self.config.temperature)
            .with_max_tokens(self.config.max_tokens)
            .with_stream(true);

        self.provider.generate_stream(req).await
    }

    /// Chat completion with tool calling support.
    ///
    /// Routes to the provider's native chat API (Ollama `/api/chat`, etc.).
    /// Falls back to prompt completion for providers that don't support tools.
    pub async fn chat(
        &self,
        messages: &[crate::provider::ChatMessage],
        tools: Option<&[crate::provider::ToolSchema]>,
    ) -> Result<crate::provider::ChatResponse> {
        self.provider.chat(messages, tools).await
    }
}
