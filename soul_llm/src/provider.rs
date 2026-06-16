use crate::error::Result;
use crate::types::{EmbeddingResult, GenerateRequest, GenerateResult, ModelInfo, StreamChunk};
use async_trait::async_trait;
use futures::stream::BoxStream;
use serde::{Deserialize, Serialize};

/// A chat message in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    System,
    User,
    Assistant,
    Tool,
}

/// A tool call requested by the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub function: ToolCallFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallFunction {
    pub name: String,
    pub arguments: String,
}

/// A tool schema advertised to the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// Response from a chat completion with possible tool calls.
#[derive(Debug, Clone)]
pub struct ChatResponse {
    pub message: ChatResponseMessage,
}

#[derive(Debug, Clone)]
pub struct ChatResponseMessage {
    pub content: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
}

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

    /// Chat completion with optional tool schemas.
    ///
    /// Default implementation flattens messages + tools into a prompt
    /// and calls `generate`. Providers should override for native tool calling.
    async fn chat(
        &self,
        messages: &[ChatMessage],
        tools: Option<&[ToolSchema]>,
    ) -> Result<ChatResponse> {
        let prompt = messages_to_prompt(messages, tools);
        let req = GenerateRequest::new(&prompt).with_stream(false);
        let result = self.generate(&req).await?;
        Ok(ChatResponse {
            message: ChatResponseMessage {
                content: Some(result.text),
                tool_calls: None,
            },
        })
    }
}

fn messages_to_prompt(messages: &[ChatMessage], _tools: Option<&[ToolSchema]>) -> String {
    let mut out = String::new();
    for m in messages {
        let role = match m.role {
            ChatRole::System => "System",
            ChatRole::User => "User",
            ChatRole::Assistant => "Assistant",
            ChatRole::Tool => "Tool result",
        };
        out.push_str(&format!("{role}: {}\n", m.content));
        if let Some(tool_calls) = &m.tool_calls {
            for tc in tool_calls {
                out.push_str(&format!(
                    "[tool call {}({})]\n",
                    tc.function.name, tc.function.arguments
                ));
            }
        }
    }
    if let Some(tools) = _tools {
        out.push_str("\nAvailable tools:\n");
        for t in tools {
            out.push_str(&format!("- {}: {}\n", t.name, t.description));
        }
    }
    out
}
