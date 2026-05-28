//! SelfQuiz — Auto-évaluation par génération de questions à partir du graphe conceptuel.
//!
//! Exécuté pendant les cycles de sommeil, SelfQuiz génère des questions
//! déterministes à partir des paires de concepts liés, compare les réponses
//! de l'agent aux connaissances stockées, et enregistre les lacunes.
//!
//! Pas d'appel LLM : les templates sont purement déterministes.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Template de question pour un type de relation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuizTemplate {
    pub template: String,
}

/// Lacune détectée.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lacuna {
    pub id: String,
    pub concept_a: String,
    pub concept_b: String,
    pub relation: String,
    pub question: String,
    pub expected_answer: String,
    pub agent_answer: String,
    pub confidence: f64,
    pub created_at: String,
}

/// Résultat d'un cycle SelfQuiz.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuizReport {
    pub questions_asked: usize,
    pub gaps_found: usize,
    pub avg_confidence: f64,
    pub lacunae: Vec<Lacuna>,
}

/// Générateur de quiz à partir du graphe conceptuel.
pub struct SelfQuiz {
    /// Templates indexés par type de relation.
    templates: HashMap<String, String>,
    /// Compteur pour IDs uniques.
    counter: u64,
}

impl Default for SelfQuiz {
    fn default() -> Self {
        let mut templates = HashMap::new();
        templates.insert("depends_on".into(), "Comment {nodeA} dépend-il de {nodeB} ?".into());
        templates.insert("implements".into(), "De quelle manière {nodeA} implémente-t-il {nodeB} ?".into());
        templates.insert("contradicts".into(), "Pourquoi {nodeA} et {nodeB} semblent-ils se contredire ?".into());
        templates.insert("similar_to".into(), "En quoi {nodeA} est-il similaire à {nodeB} ?".into());
        templates.insert("part_of".into(), "Comment {nodeA} s'intègre-t-il dans {nodeB} ?".into());
        Self { templates, counter: 0 }
    }
}

impl SelfQuiz {
    pub fn new() -> Self {
        Self::default()
    }

    /// Ajoute ou remplace un template pour un type de relation.
    pub fn add_template(&mut self, relation: &str, template: &str) {
        self.templates.insert(relation.to_string(), template.to_string());
    }

    /// Génère une question à partir de deux concepts et leur relation.
    pub fn generate_question(&self, concept_a: &str, concept_b: &str, relation: &str) -> Option<String> {
        self.templates.get(relation).map(|t| {
            t.replace("{nodeA}", concept_a).replace("{nodeB}", concept_b)
        })
    }

    /// Simule un cycle de quiz complet.
    ///
    /// Dans une implémentation réelle, cette méthode interrogerait le graphe
    /// conceptuel, générerait des questions, et comparerait les réponses
    /// de l'agent aux connaissances stockées.
    pub fn run_quiz(&mut self, pairs: &[(String, String, String)]) -> QuizReport {
        let mut lacunae = Vec::new();
        let mut total_confidence = 0.0;

        for (concept_a, concept_b, relation) in pairs {
            self.counter += 1;
            if let Some(question) = self.generate_question(concept_a, concept_b, relation) {
                // Simulation : agent répond avec une confiance imparfaite
                let expected = format!("{} → {}", concept_a, relation);
                let agent_answer = format!("{} (via {})", concept_b, relation);
                let confidence = 0.7 + (self.counter as f64 * 0.01).fract(); // simule variabilité
                total_confidence += confidence;

                if confidence < 0.8 {
                    lacunae.push(Lacuna {
                        id: format!("lacuna-{}", self.counter),
                        concept_a: concept_a.clone(),
                        concept_b: concept_b.clone(),
                        relation: relation.clone(),
                        question,
                        expected_answer: expected,
                        agent_answer,
                        confidence,
                        created_at: chrono::Utc::now().to_rfc3339(),
                    });
                }
            }
        }

        let count = pairs.len().max(1);
        QuizReport {
            questions_asked: pairs.len(),
            gaps_found: lacunae.len(),
            avg_confidence: total_confidence / count as f64,
            lacunae,
        }
    }

    /// Enregistre les lacunes dans un fichier JSON.
    pub fn save_lacunae(&self, lacunae: &[Lacuna], path: &str) -> Result<(), String> {
        let json = serde_json::to_string_pretty(lacunae).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Charge les lacunes depuis un fichier JSON.
    pub fn load_lacunae(path: &str) -> Result<Vec<Lacuna>, String> {
        let data = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        serde_json::from_str(&data).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_question() {
        let quiz = SelfQuiz::default();
        let q = quiz.generate_question("encodeur", "décodeur", "depends_on");
        assert!(q.is_some());
        assert!(q.unwrap().contains("encodeur"));
    }

    #[test]
    fn test_unknown_relation() {
        let quiz = SelfQuiz::default();
        let q = quiz.generate_question("a", "b", "unknown_relation");
        assert!(q.is_none());
    }

    #[test]
    fn test_run_quiz() {
        let mut quiz = SelfQuiz::new();
        let pairs = vec![
            ("encodeur".into(), "décodeur".into(), "depends_on".into()),
            ("concept".into(), "instance".into(), "implements".into()),
        ];
        let report = quiz.run_quiz(&pairs);
        assert_eq!(report.questions_asked, 2);
    }

    #[test]
    fn test_add_template() {
        let mut quiz = SelfQuiz::new();
        quiz.add_template("causes", "{nodeA} est-il une cause de {nodeB} ?");
        let q = quiz.generate_question("feu", "fumée", "causes");
        assert!(q.is_some());
    }
}
