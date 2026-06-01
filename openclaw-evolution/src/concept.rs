use crate::ranking::cosine_similarity;
use rand::Rng;
use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────
// Concept avec embedding sémantique
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Concept {
    pub identity: String,
    pub transformation_rule: String,
    pub stability: f32,
    pub generation: u64,
    pub parent_concepts: Vec<String>,
    /// Vecteur d'embedding sémantique (optionnel)
    pub embedding: Option<Vec<f32>>,
    /// Nombre de fois que ce concept a contribué à une amélioration
    pub success_count: u64,
    /// Nombre total d'utilisations
    pub usage_count: u64,
}

impl Concept {
    pub fn new(identity: &str, transformation_rule: &str) -> Self {
        Self {
            identity: identity.to_string(),
            transformation_rule: transformation_rule.to_string(),
            stability: 0.5,
            generation: 0,
            parent_concepts: Vec::new(),
            embedding: None,
            success_count: 0,
            usage_count: 0,
        }
    }

    /// Définir l'embedding sémantique
    pub fn set_embedding(&mut self, embedding: Vec<f32>) {
        self.embedding = Some(embedding);
    }

    /// Taux de succès réel
    pub fn success_rate(&self) -> f32 {
        if self.usage_count == 0 {
            return 0.5; // prior neutre
        }
        self.success_count as f32 / self.usage_count as f32
    }

    /// Rapporter un résultat d'utilisation
    pub fn report_usage(&mut self, success: bool) {
        self.usage_count += 1;
        if success {
            self.success_count += 1;
        }
        // Ajuster la stabilité basée sur le taux de succès réel
        self.stability = self.stability * 0.8 + self.success_rate() * 0.2;
    }

    /// Similarité sémantique avec un autre concept
    pub fn similarity_to(&self, other: &Concept) -> f32 {
        match (&self.embedding, &other.embedding) {
            (Some(a), Some(b)) => cosine_similarity(a, b),
            _ => 0.0,
        }
    }

    /// Faire muter le concept
    pub fn mutate(&mut self, feedback: f32) {
        self.stability = (self.stability + (feedback - 0.5) * 0.15).clamp(0.0, 1.0);
        self.generation += 1;
    }

    /// Fusionner deux concepts en combinant leurs embeddings
    pub fn merge(a: &Concept, b: &Concept) -> Concept {
        // Combiner les embeddings par moyenne
        let merged_embedding = match (&a.embedding, &b.embedding) {
            (Some(ea), Some(eb)) if ea.len() == eb.len() => {
                let avg: Vec<f32> = ea
                    .iter()
                    .zip(eb.iter())
                    .map(|(x, y)| (x + y) / 2.0)
                    .collect();
                Some(avg)
            }
            (Some(e), None) | (None, Some(e)) => Some(e.clone()),
            _ => None,
        };

        Concept {
            identity: format!("{}+{}", a.identity, b.identity),
            transformation_rule: format!(
                "({}) ∘ ({})",
                a.transformation_rule, b.transformation_rule
            ),
            stability: (a.stability + b.stability) / 2.0,
            generation: a.generation.max(b.generation) + 1,
            parent_concepts: vec![a.identity.clone(), b.identity.clone()],
            embedding: merged_embedding,
            success_count: 0,
            usage_count: 0,
        }
    }
}

// ─────────────────────────────────────────────
// Moteur de concepts
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConceptEngine {
    pub concepts: Vec<Concept>,
    pub max_concepts: usize,
}

impl ConceptEngine {
    pub fn new(max_concepts: usize) -> Self {
        Self {
            concepts: vec![
                Concept::new("evolution", "iterate → mutate → select → repeat"),
                Concept::new("exploration", "search → embed → rank → store"),
                Concept::new("optimization", "measure → identify → improve → validate"),
                Concept::new("stability", "snapshot → validate → rollback_if_fail"),
                Concept::new("memory", "store → prune → compress → retrieve"),
                Concept::new("emergence", "combine → observe → reinforce → abstract"),
            ],
            max_concepts,
        }
    }

    pub fn find(&self, identity: &str) -> Option<&Concept> {
        self.concepts.iter().find(|c| c.identity == identity)
    }

    pub fn find_mut(&mut self, identity: &str) -> Option<&mut Concept> {
        self.concepts.iter_mut().find(|c| c.identity == identity)
    }

    /// Trouver le concept le plus pertinent par embedding
    pub fn find_most_relevant(&self, query_embedding: &[f32]) -> Option<&Concept> {
        self.concepts
            .iter()
            .filter(|c| c.embedding.is_some())
            .max_by(|a, b| {
                let sim_a = cosine_similarity(query_embedding, a.embedding.as_ref().unwrap());
                let sim_b = cosine_similarity(query_embedding, b.embedding.as_ref().unwrap());
                sim_a.partial_cmp(&sim_b).unwrap()
            })
    }

    /// Évoluer tous les concepts
    pub fn evolve_all(&mut self, global_feedback: f32) {
        for concept in &mut self.concepts {
            concept.mutate(global_feedback);
        }
        self.try_merge_similar();
        self.prune();
    }

    /// Fusionner les concepts sémantiquement proches
    fn try_merge_similar(&mut self) {
        if self.concepts.len() < 2 || self.concepts.len() >= self.max_concepts {
            return;
        }

        let mut rng = rand::thread_rng();
        if !rng.gen_bool(0.08) {
            return;
        }

        // Trouver la paire la plus similaire
        let mut best_sim = 0.0f32;
        let mut best_pair = (0, 1);

        for i in 0..self.concepts.len() {
            for j in (i + 1)..self.concepts.len() {
                let sim = self.concepts[i].similarity_to(&self.concepts[j]);
                if sim > best_sim && sim > 0.7 {
                    best_sim = sim;
                    best_pair = (i, j);
                }
            }
        }

        if best_sim > 0.7 {
            let merged = Concept::merge(&self.concepts[best_pair.0], &self.concepts[best_pair.1]);
            tracing::info!(
                "Concept fusionné: {} + {} → {} (sim={:.3})",
                self.concepts[best_pair.0].identity,
                self.concepts[best_pair.1].identity,
                merged.identity,
                best_sim
            );
            self.concepts.push(merged);
        }
    }

    fn prune(&mut self) {
        // Garder les concepts avec un taux de succès décent ou peu testés
        self.concepts
            .retain(|c| c.stability > 0.1 || c.usage_count < 3);

        if self.concepts.len() > self.max_concepts {
            self.concepts
                .sort_by(|a, b| b.stability.partial_cmp(&a.stability).unwrap());
            self.concepts.truncate(self.max_concepts);
        }
    }

    pub fn active_concepts(&self) -> Vec<String> {
        self.concepts.iter().map(|c| c.identity.clone()).collect()
    }

    pub fn average_stability(&self) -> f32 {
        if self.concepts.is_empty() {
            return 0.0;
        }
        self.concepts.iter().map(|c| c.stability).sum::<f32>() / self.concepts.len() as f32
    }
}
