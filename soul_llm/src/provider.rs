use crate::error::Result;
use crate::types::{EmbeddingResult, GenerateRequest, GenerateResult, ModelInfo, StreamChunk};
use async_trait::async_trait;
use futures::stream::BoxStream;

/// Trait unifié pour tous les providers LLM.
///
/// Chaque provider (Ollama, OpenAI, Anthropic) implémente ce trait.
/// Le workspace utilise uniquement ce trait — jamais les types concrets.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Nom du provider (ex: "ollama", "openai", "anthropic").
    fn name(&self) -> &str;

    /// Vérifie que le service est joignable.
    async fn health_check(&self) -> bool;

    /// Liste les modèles disponibles.
    async fn list_models(&self) -> Result<Vec<ModelInfo>>;

    /// Génération batch (réponse complète).
    async fn generate(&self, request: &GenerateRequest) -> Result<GenerateResult>;

    /// Génération streaming (chunks en temps réel).
    ///
    /// Retourne un flux de `StreamChunk`. Le dernier chunk a `done: true`
    /// et éventuellement les `usage` finaux.
    async fn generate_stream(
        &self,
        request: GenerateRequest,
    ) -> Result<BoxStream<'static, Result<StreamChunk>>>;

    /// Génération d'embedding pour un texte.
    async fn embed(&self, text: &str, model: Option<&str>) -> Result<EmbeddingResult>;

    /// Génération d'embeddings par batch.
    ///
    /// Par défaut, appelle `embed()` séquentiellement. Les providers
    /// peuvent surcharger pour utiliser des APIs batch natifs.
    async fn embed_batch<'a>(
        &'a self,
        texts: &[&str],
        model: Option<&str>,
    ) -> Result<Vec<EmbeddingResult>> {
        let mut results = Vec::with_capacity(texts.len());
        for text in texts {
            results.push(self.embed(text, model).await?);
        }
        Ok(results)
    }
}
