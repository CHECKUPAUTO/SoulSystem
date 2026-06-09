//! LLM Client — Client Ollama unifié pour chat, embeddings et structured output.

use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{info, warn};

/// Client unifié pour l'API Ollama (chat + embeddings).
#[derive(Debug, Clone)]
pub struct OllamaClient {
    base_url: String,
    model: String,
    client: reqwest::Client,
}

// ── Types de requête/réponse ─────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    format: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    message: ChatResponseMessage,
}

#[derive(Debug, Deserialize)]
struct ChatResponseMessage {
    content: String,
}

#[derive(Debug, Serialize)]
struct EmbeddingRequest {
    model: String,
    prompt: String,
}

#[derive(Debug, Deserialize)]
pub struct EmbeddingResponse {
    pub embedding: Vec<f32>,
}

// ── Réponse LLM structurée ───────────────────────────────────────────────

const VALID_ACTIONS: [&str; 10] = [
    "optimize_system",
    "restart_service",
    "investigate",
    "evolve",
    "wait",
    "explore",
    "alert",
    "block_ip",
    "tune_gpu",
    "verbalize",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmResponse {
    pub thought: String,
    pub action: String,
    pub confidence: f32,
    pub reasoning: String,
}

impl LlmResponse {
    /// Valide la réponse du LLM selon un schema strict.
    pub fn validate(&self) -> Result<(), String> {
        if !VALID_ACTIONS.contains(&self.action.as_str()) {
            return Err(format!(
                "Action invalide: '{}'. Attendu: {:?}",
                self.action, VALID_ACTIONS
            ));
        }
        if self.confidence < 0.0 || self.confidence > 1.0 {
            return Err(format!(
                "Confidence hors limites: {}. Attendu: [0.0, 1.0]",
                self.confidence
            ));
        }
        if self.thought.is_empty() {
            return Err("thought est vide".to_string());
        }
        if self.thought.len() > 1000 {
            return Err(format!(
                "thought trop long: {} chars (max 1000)",
                self.thought.len()
            ));
        }
        if self.reasoning.is_empty() {
            return Err("reasoning est vide".to_string());
        }
        if self.reasoning.len() > 500 {
            return Err(format!(
                "reasoning trop long: {} chars (max 500)",
                self.reasoning.len()
            ));
        }
        Ok(())
    }

    /// Score de priorité (urgence × impact × énergie).
    pub fn priority_score(&self, cpu: f32, mem: f32) -> f32 {
        let base = self.confidence;
        let urgency = if cpu > 80.0 || mem > 90.0 {
            2.0
        } else if cpu > 50.0 || mem > 70.0 {
            1.5
        } else {
            1.0
        };
        let impact = match self.action.as_str() {
            "optimize_system" => 1.2,
            "restart_service" => 1.5,
            "investigate" => 1.0,
            "evolve" => 1.4,
            "block_ip" => 1.8,
            "tune_gpu" => 1.1,
            "verbalize" => 0.9,
            "wait" => 0.5,
            "explore" => 0.9,
            "alert" => 1.3,
            _ => 1.0,
        };
        base * urgency * impact
    }
}

// ── Implémentation OllamaClient ──────────────────────────────────────────

impl OllamaClient {
    pub fn new(base_url: &str, model: &str) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .expect("failed to build reqwest client");
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            model: model.to_string(),
            client,
        }
    }

    pub fn with_client(base_url: &str, model: &str, client: reqwest::Client) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            model: model.to_string(),
            client,
        }
    }

    pub fn model_name(&self) -> &str {
        &self.model
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Envoie un chat complet à Ollama et retourne le contenu texte.
    pub async fn chat(&self, messages: Vec<ChatMessage>) -> anyhow::Result<String> {
        let request = ChatRequest {
            model: self.model.clone(),
            messages,
            stream: false,
            format: None,
        };
        let url = format!("{}/api/chat", self.base_url);
        let resp = self.client.post(&url).json(&request).send().await?;
        let body: ChatResponse = resp.json().await?;
        Ok(body.message.content)
    }

    /// Envoie un chat avec format JSON forcé.
    pub async fn chat_json(&self, messages: Vec<ChatMessage>) -> anyhow::Result<String> {
        let request = ChatRequest {
            model: self.model.clone(),
            messages,
            stream: false,
            format: Some("json".to_string()),
        };
        let url = format!("{}/api/chat", self.base_url);
        let resp = self.client.post(&url).json(&request).send().await?;
        let body: ChatResponse = resp.json().await?;
        Ok(body.message.content)
    }

    /// Obtient un embedding vectoriel pour un texte.
    pub async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        let request = EmbeddingRequest {
            model: self.model.clone(),
            prompt: text.to_string(),
        };
        let url = format!("{}/api/embeddings", self.base_url);
        let resp = self.client.post(&url).json(&request).send().await?;
        let body: EmbeddingResponse = resp.json().await?;
        Ok(body.embedding)
    }

    /// Retire les code fences markdown d'une réponse LLM.
    pub fn strip_code_fences(content: &str) -> &str {
        let content = content.trim();
        let content = if let Some(rest) = content.strip_prefix("```json") {
            rest
        } else if let Some(rest) = content.strip_prefix("```") {
            rest
        } else {
            content
        };
        let content = if let Some(rest) = content.strip_suffix("```") {
            rest
        } else {
            content
        };
        content.trim()
    }

    /// Demande une sortie structurée JSON au LLM.
    pub async fn structured_output<T: serde::de::DeserializeOwned>(
        &self,
        system_prompt: &str,
        user_prompt: &str,
    ) -> anyhow::Result<T> {
        let messages = vec![
            ChatMessage {
                role: "system".into(),
                content: system_prompt.into(),
            },
            ChatMessage {
                role: "user".into(),
                content: user_prompt.into(),
            },
        ];
        let raw = self.chat_json(messages).await?;
        let cleaned = Self::strip_code_fences(&raw);
        let parsed = serde_json::from_str::<T>(cleaned)
            .map_err(|e| anyhow::anyhow!("JSON parse error: {e}\nRaw: {raw}"))?;
        Ok(parsed)
    }

    /// Réflexion LLM avec réponse structurée LlmResponse.
    pub async fn reflect(
        &self,
        context: &str,
        goal: &str,
        system_state: &str,
    ) -> Option<LlmResponse> {
        let prompt = format!(
            r#"Tu es l'intelligence centrale d'un organisme distribué.
Contexte système: {context}
<Objective>{goal}</Objective>
État physique: {system_state}

Réponds UNIQUEMENT en JSON:
{{
  "thought": "ton analyse",
  "action": "optimize_system | restart_service | investigate | evolve | explore | alert | wait | block_ip | tune_gpu | verbalize",
  "confidence": 0.0-1.0,
  "reasoning": "justification"
}}"#
        );

        let messages = vec![ChatMessage {
            role: "user".into(),
            content: prompt,
        }];

        match self.chat_json(messages).await {
            Ok(raw) => {
                let cleaned = Self::strip_code_fences(&raw);
                match serde_json::from_str::<LlmResponse>(cleaned) {
                    Ok(parsed) => match parsed.validate() {
                        Ok(_) => {
                            info!(
                                "LLM: {} | conf={:.2} | {}",
                                parsed.action,
                                parsed.confidence,
                                parsed.thought.chars().take(60).collect::<String>()
                            );
                            Some(parsed)
                        }
                        Err(e) => {
                            warn!("LLM validation error: {}", e);
                            None
                        }
                    },
                    Err(e) => {
                        warn!("LLM parse error: {}", e);
                        None
                    }
                }
            }
            Err(e) => {
                warn!("LLM request error: {}", e);
                None
            }
        }
    }

    /// Génère du code Rust via LLM.
    pub async fn generate_code(
        &self,
        file_path: &str,
        issue: &str,
        current_code: &str,
    ) -> Option<String> {
        let prompt = format!(
            r#"Tu es un programmeur Rust. Fichier: {file_path}
Problème: {issue}
Code original:
```rust
{current_code}
```
Réponds en JSON: {{"code": "le code complet"}}"#
        );

        let messages = vec![ChatMessage {
            role: "user".into(),
            content: prompt,
        }];

        match self.chat_json(messages).await {
            Ok(raw) => {
                let cleaned = Self::strip_code_fences(&raw);
                match serde_json::from_str::<serde_json::Value>(cleaned) {
                    Ok(parsed) => {
                        let code = parsed
                            .get("code")
                            .and_then(|c| c.as_str())
                            .unwrap_or("")
                            .trim()
                            .to_string();

                        if !code.is_empty() && code.contains("fn ") {
                            info!("LLM code generated: {} octets", code.len());
                            Some(code)
                        } else {
                            warn!("Generated code invalid: {} octets", code.len());
                            None
                        }
                    }
                    Err(e) => {
                        warn!("LLM JSON parse error: {}", e);
                        None
                    }
                }
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_code_fences_json() {
        let input = "```json\n{\"key\": \"value\"}\n```";
        assert_eq!(OllamaClient::strip_code_fences(input), "{\"key\": \"value\"}");
    }

    #[test]
    fn test_strip_code_fences_plain() {
        let input = "{\"key\": \"value\"}";
        assert_eq!(OllamaClient::strip_code_fences(input), "{\"key\": \"value\"}");
    }

    #[test]
    fn test_strip_code_fences_no_fence() {
        let input = "  some text  ";
        assert_eq!(OllamaClient::strip_code_fences(input), "some text");
    }

    #[test]
    fn test_client_construction() {
        let c = OllamaClient::new("http://localhost:11434", "test-model");
        assert_eq!(c.base_url, "http://localhost:11434");
        assert_eq!(c.model, "test-model");
    }

    #[test]
    fn test_client_trims_trailing_slash() {
        let c = OllamaClient::new("http://localhost:11434/", "test");
        assert_eq!(c.base_url, "http://localhost:11434");
    }

    #[test]
    fn test_valid_response() {
        let resp = LlmResponse {
            thought: "test".into(),
            action: "wait".into(),
            confidence: 0.8,
            reasoning: "because".into(),
        };
        assert!(resp.validate().is_ok());
    }

    #[test]
    fn test_invalid_action() {
        let resp = LlmResponse {
            thought: "test".into(),
            action: "invalid_action".into(),
            confidence: 0.8,
            reasoning: "because".into(),
        };
        assert!(resp.validate().is_err());
    }

    #[test]
    fn test_priority_score_urgent() {
        let resp = LlmResponse {
            thought: "test".into(),
            action: "restart_service".into(),
            confidence: 0.9,
            reasoning: "because".into(),
        };
        let score = resp.priority_score(90.0, 95.0);
        assert!(score > 2.0);
    }
}
