//! Model Router — Selection dynamique du modele Ollama selon complexite.
//!
//! Evalue la complexite d'une requete par heuristique (longueur, mots-cles)
//! et choisit le modele le moins couteux ayant la capacite suffisante.

use serde::{Deserialize, Serialize};

/// Modele disponible avec sa capacite estimee (0.0–1.0) et son cout relatif.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSpec {
    pub name: String,
    /// Capacite estimee du modele (0.0 = simple, 1.0 = tres capable).
    pub capacity: f64,
    /// Cout relatif (1.0 = reference).
    pub cost: f64,
}

/// Routeur de modeles.
pub struct ModelRouter {
    models: Vec<ModelSpec>,
}

impl ModelRouter {
    /// Cree un routeur avec une liste de modeles.
    /// Les modeles doivent etre tries par cout croissant.
    pub fn new(models: Vec<ModelSpec>) -> Self {
        let mut sorted = models;
        sorted.sort_by(|a, b| {
            a.cost
                .partial_cmp(&b.cost)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Self { models: sorted }
    }

    /// Cree un routeur avec les modeles par defaut.
    pub fn default_models() -> Self {
        Self::new(vec![
            ModelSpec {
                name: "kimi-k2.6:cloud".into(),
                capacity: 1.0,
                cost: 0.1,
            },
            ModelSpec {
                name: "qwen3:4b".into(),
                capacity: 0.4,
                cost: 0.2,
            },
            ModelSpec {
                name: "deepseek-coder-v2:16b".into(),
                capacity: 0.95,
                cost: 0.5,
            },
        ])
    }

    /// Evalue la complexite d'une requete (0.0 = simple, 1.0 = complexe).
    pub fn complexity(&self, query: &str) -> f64 {
        let mut score = 0.0;
        let lower = query.to_lowercase();
        let len = query.len() as f64;

        // Longueur: requetes plus longues = plus complexes
        score += (len / 500.0).min(0.3);

        // Mots-cles de complexite
        let complex_keywords = [
            ("explique", 0.15),
            ("explain", 0.15),
            ("code", 0.2),
            ("implemente", 0.2),
            ("implement", 0.2),
            ("architecture", 0.2),
            ("debug", 0.2),
            ("optimise", 0.2),
            ("optimize", 0.2),
            ("refactor", 0.2),
            ("resume", 0.1),
            ("summarize", 0.1),
            ("analyse", 0.15),
            ("analyze", 0.15),
            ("securite", 0.2),
            ("security", 0.2),
            ("conception", 0.15),
            ("design", 0.15),
            ("traduis", 0.1),
            ("translate", 0.1),
        ];

        for (kw, weight) in &complex_keywords {
            if lower.contains(kw) {
                score += weight;
            }
        }

        // Ponctuation complexe
        let code_indicators = ['{', '}', '(', ')', '[', ']', ';', '=', '<', '>', '|', '&'];
        let code_count = query
            .chars()
            .filter(|c| code_indicators.contains(c))
            .count();
        score += (code_count as f64 / 20.0).min(0.15);

        score.clamp(0.0, 1.0)
    }

    /// Selectionne le modele le moins couteux capable de traiter la requete.
    /// Retourne le nom du modele.
    pub fn route(&self, query: &str) -> &str {
        let complexity = self.complexity(query);
        for model in &self.models {
            if model.capacity >= complexity {
                tracing::debug!(
                    "ModelRouter: complexity={:.2} → {} (cap={:.2}, cost={:.2})",
                    complexity,
                    model.name,
                    model.capacity,
                    model.cost
                );
                return &model.name;
            }
        }
        // Fallback: modele le plus capable
        let best = self.models.last().unwrap();
        tracing::debug!(
            "ModelRouter: complexity={:.2} → fallback {}",
            complexity,
            best.name
        );
        &best.name
    }

    /// Retourne la liste des modeles disponibles.
    pub fn models(&self) -> &[ModelSpec] {
        &self.models
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_query_routes_to_small_model() {
        let router = ModelRouter::default_models();
        let model = router.route("bonjour");
        assert!(
            model == "tinyllama" || model == "llama3.2:1b",
            "simple query should use small model, got {}",
            model
        );
    }

    #[test]
    fn test_complex_query_routes_to_large_model() {
        let router = ModelRouter::default_models();
        let model = router.route(
            "explique l'architecture du code et implemente une solution securisee \
             pour le probleme d'authentification avec OAuth2",
        );
        // Should route to at least mistral-level
        assert!(
            model != "tinyllama",
            "complex query should not use tinyllama, got {}",
            model
        );
    }

    #[test]
    fn test_code_query_routes_high() {
        let router = ModelRouter::default_models();
        let model = router.route("code: fn main() { let x = vec![1,2,3]; } implemente le tri");
        let cap = router
            .models()
            .iter()
            .find(|m| m.name == model)
            .map(|m| m.capacity)
            .unwrap_or(0.0);
        assert!(
            cap >= 0.5,
            "code query needs medium+ model, got {} (cap={})",
            model,
            cap
        );
    }

    #[test]
    fn test_complexity_range() {
        let router = ModelRouter::default_models();
        let c_empty = router.complexity("");
        let c_simple = router.complexity("bonjour");
        let c_complex = router.complexity(
            "explique l'architecture securisee et implemente le code pour un systeme distribue",
        );
        assert!(c_empty < c_simple);
        assert!(c_simple < c_complex);
        assert!(c_empty >= 0.0);
        assert!(c_complex <= 1.0);
    }
}
