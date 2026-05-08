//! LLM — Réflexion avancée via Ollama avec validation schema

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

const VALID_ACTIONS: [&str; 7] = ["optimize_system", "restart_service", "investigate", "evolve", "wait", "explore", "alert"];

pub struct LlmEngine {
    url: String,
    model: String,
    client: reqwest::Client,
}

impl Clone for LlmEngine {
    fn clone(&self) -> Self {
        Self {
            url: self.url.clone(),
            model: self.model.clone(),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmResponse {
    pub thought: String,
    pub action: String,
    pub confidence: f32,
    pub reasoning: String,
}

impl LlmResponse {
    /// Valide la réponse du LLM selon un schema strict
    pub fn validate(&self) -> Result<(), String> {
        // action ∈ ensemble valide
        if !VALID_ACTIONS.contains(&self.action.as_str()) {
            return Err(format!("Action invalide: '{}'. Attendu: {:?}", self.action, VALID_ACTIONS));
        }

        // confidence ∈ [0.0, 1.0]
        if self.confidence < 0.0 || self.confidence > 1.0 {
            return Err(format!("Confidence hors limites: {}. Attendu: [0.0, 1.0]", self.confidence));
        }

        // thought non vide (max 1000 chars)
        if self.thought.is_empty() {
            return Err("thought est vide".to_string());
        }
        if self.thought.len() > 1000 {
            return Err(format!("thought trop long: {} chars (max 1000)", self.thought.len()));
        }

        // reasoning non vide (max 500 chars)
        if self.reasoning.is_empty() {
            return Err("reasoning est vide".to_string());
        }
        if self.reasoning.len() > 500 {
            return Err(format!("reasoning trop long: {} chars (max 500)", self.reasoning.len()));
        }

        Ok(())
    }

    /// Convertit l'action en score de priorité (urgence × impact × énergie)
    pub fn priority_score(&self, cpu: f32, mem: f32, disk: f32) -> f32 {
        let base = self.confidence;
        let urgency = if cpu > 80.0 || mem > 90.0 { 2.0 } else if cpu > 50.0 || mem > 70.0 { 1.5 } else { 1.0 };
        let impact = match self.action.as_str() {
            "optimize_system" => 1.2,
            "restart_service" => 1.5,
            "investigate" => 1.0,
            "evolve" => 1.4,
            "wait" => 0.5,
            "explore" => 0.9,
            "alert" => 1.3,
            _ => 1.0,
        };
        base * urgency * impact
    }
}

impl LlmEngine {
    pub fn new(url: &str, model: &str) -> Self {
        Self {
            url: url.trim_end_matches('/').to_string(),
            model: model.to_string(),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
        }
    }

    pub fn model_name(&self) -> &str {
        &self.model
    }

    pub async fn reflect(
        &self,
        context: &str,
        goal: &str,
        system_state: &str,
    ) -> Option<LlmResponse> {
        let prompt = format!(
            r#"Tu es OpenClaw-U, l'intelligence centrale d'un organisme distribué composé de 11 organes (Science, Mind, Engineer, Crypto, Creative, Meta, Foresight, Homeostasis, Creativity, Social, Validation).
Ta mission est d'assurer la survie, l'évolution et l'expansion de l'organisme.

Contexte système (HNN Mesh):
{}

But actuel:
{}

État physique (Hardware):
{}

Réponds UNIQUEMENT en JSON avec ce format exact:
{{
  "thought": "ton analyse métacognitive de la situation",
  "action": "optimize_system | restart_service | investigate | evolve | explore | alert | wait",
  "confidence": 0.0-1.0,
  "reasoning": "justification logique"
}}"#,
            context, goal, system_state
        );

        let body = serde_json::json!({
            "model": self.model,
            "messages": [{"role": "user", "content": prompt}],
            "stream": false,
            "format": "json"
        });

        let resp = self.client
            .post(format!("{}/api/chat", self.url))
            .json(&body)
            .send().await;

        match resp {
            Ok(r) if r.status().is_success() => {
                match r.json::<serde_json::Value>().await {
                    Ok(json) => {
                        let content = json
                            .get("message")
                            .and_then(|m| m.get("content"))
                            .and_then(|c| c.as_str())
                            .unwrap_or("");

                        match serde_json::from_str::<LlmResponse>(content) {
                            Ok(parsed) => {
                                // Validation schema strict
                                match parsed.validate() {
                                    Ok(_) => {
                                        info!("🧠 LLM: {} | conf={:.2} | {}", parsed.action, parsed.confidence, parsed.thought.chars().take(60).collect::<String>());
                                        Some(parsed)
                                    }
                                    Err(e) => {
                                        warn!("❌ LLM validation error: {}", e);
                                        None
                                    }
                                }
                            }
                            Err(e) => {
                                warn!("❌ LLM parse error: {}", e);
                                None
                            }
                        }
                    }
                    Err(e) => {
                        warn!("❌ LLM JSON error: {}", e);
                        None
                    }
                }
            }
            Ok(r) => {
                warn!("❌ LLM HTTP error: {}", r.status());
                None
            }
            Err(e) => {
                warn!("❌ LLM request error: {}", e);
                None
            }
        }
    }

    pub async fn generate_code(
        &self,
        file_path: &str,
        issue: &str,
        current_code: &str,
    ) -> Option<String> {
        let prompt = format!(
            r#"Tu es un programmeur Rust. Fichier à corriger: {}

RÈGLES ABSOLUES:
1. Réponds UNIQUEMENT en JSON
2. Format exact: {{"code": "..."}}
3. Le code doit être du Rust COMPLET (tous les imports, tous les modules)
4. PAS d'explication, PAS de markdown, PAS de commentaires inutiles
5. Le code doit compiler avec `cargo check`
6. Garde la structure originale: attributs derives, imports, structs
7. Remplace UNIQUEMENT les unwrap() par expect("message descriptif")
8. N'ajoute PAS de dépendances externes

Problème: {}

Code original:
```rust
{}
```

Réponds en JSON: {{"code": "le code complet ici"}}"#,
            file_path, issue, current_code
        );

        let body = serde_json::json!({
            "model": self.model,
            "messages": [{"role": "user", "content": prompt}],
            "stream": false,
            "format": "json"
        });

        let resp = self.client
            .post(format!("{}/api/chat", self.url))
            .json(&body)
            .send().await;

        match resp {
            Ok(r) if r.status().is_success() => {
                match r.json::<serde_json::Value>().await {
                    Ok(json) => {
                        let content = json
                            .get("message")
                            .and_then(|m| m.get("content"))
                            .and_then(|c| c.as_str())
                            .unwrap_or("");

                        // Parse JSON response
                        match serde_json::from_str::<serde_json::Value>(content) {
                            Ok(parsed) => {
                                let code = parsed
                                    .get("code")
                                    .and_then(|c| c.as_str())
                                    .unwrap_or("")
                                    .trim()
                                    .to_string();

                                if !code.is_empty()
                                    && code.contains("fn ")
                                    && code.len() > current_code.len() / 2
                                {
                                    info!("✨ LLM généré: {} octets", code.len());
                                    Some(code)
                                } else {
                                    warn!("❌ Code généré invalide: {} octets", code.len());
                                    None
                                }
                            }
                            Err(e) => {
                                warn!("❌ LLM JSON parse error: {}", e);
                                None
                            }
                        }
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }
}
