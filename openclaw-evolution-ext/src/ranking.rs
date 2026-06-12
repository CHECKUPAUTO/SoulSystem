use crate::crawler::CrawlResult;
use crate::ollama::OllamaEngine;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

// ─────────────────────────────────────────────
// Résultat classé
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankedResult {
    pub crawl: CrawlResult,
    pub semantic_score: f32,
    pub keyword_score: f32,
    pub novelty_score: f32,
    pub structural_quality: f32,
    pub total_score: f32,
    /// Vecteur d'embedding du contenu
    pub embedding: Option<Vec<f32>>,
}

// ─────────────────────────────────────────────
// Moteur de classement avec embeddings
// ─────────────────────────────────────────────

pub struct RankingEngine {
    focus_keywords: Vec<String>,
    seen_hashes: Vec<String>,
    top_k: usize,
    /// Embedding de la requête de curiosité courante
    query_embedding: Option<Vec<f32>>,
}

impl RankingEngine {
    pub fn new(top_k: usize) -> Self {
        Self {
            focus_keywords: vec![
                "rust".into(), "ai".into(), "machine learning".into(),
                "llm".into(), "optimization".into(), "evolution".into(),
                "algorithm".into(), "agent".into(), "performance".into(),
                "async".into(), "concurrent".into(), "memory".into(),
            ],
            seen_hashes: Vec::new(),
            top_k,
            query_embedding: None,
        }
    }

    pub fn add_keywords(&mut self, keywords: Vec<String>) {
        self.focus_keywords.extend(keywords);
    }

    /// Définir l'embedding de la requête courante pour le scoring sémantique
    pub fn set_query_embedding(&mut self, embedding: Vec<f32>) {
        self.query_embedding = Some(embedding);
    }

    /// Classer avec embeddings sémantiques (appel async pour chaque résultat)
    pub async fn rank_with_embeddings(
        &mut self,
        results: Vec<CrawlResult>,
        ollama: &OllamaEngine,
    ) -> Vec<RankedResult> {
        let mut ranked: Vec<RankedResult> = Vec::new();

        for crawl in results {
            let keyword_score = self.compute_keyword_score(&crawl);
            let novelty_score = self.compute_novelty(&crawl);
            let structural_quality = self.compute_structural_quality(&crawl);

            // Générer l'embedding du contenu
            let content_text = format!("{} {}", crawl.title, &crawl.content[..crawl.content.len().min(500)]);
            let embedding = match ollama.embed(&content_text).await {
                Ok(emb) => Some(emb),
                Err(e) => {
                    warn!("Embedding échoué pour {}: {}", crawl.url, e);
                    None
                }
            };

            // Score sémantique par cosine similarity
            let semantic_score = match (&self.query_embedding, &embedding) {
                (Some(query), Some(content)) => cosine_similarity(query, content),
                _ => 0.0,
            };

            // Score total : sémantique pèse lourd
            let total_score = semantic_score * 0.4
                + keyword_score * 0.2
                + novelty_score * 0.2
                + structural_quality * 0.2;

            ranked.push(RankedResult {
                crawl,
                semantic_score,
                keyword_score,
                novelty_score,
                structural_quality,
                total_score,
                embedding,
            });
        }

        ranked.sort_by(|a, b| b.total_score.partial_cmp(&a.total_score).unwrap());
        ranked.truncate(self.top_k);

        // Marquer comme vu
        for r in &ranked {
            let hash = content_hash(&r.crawl.content);
            self.seen_hashes.push(hash);
        }

        info!(
            "Ranking: {} résultats classés (top score={:.3})",
            ranked.len(),
            ranked.first().map(|r| r.total_score).unwrap_or(0.0)
        );

        ranked
    }

    /// Fallback sans embeddings (si Ollama embed est indisponible)
    pub fn rank_keywords_only(&mut self, results: Vec<CrawlResult>) -> Vec<RankedResult> {
        let mut ranked: Vec<RankedResult> = results
            .into_iter()
            .map(|crawl| {
                let keyword_score = self.compute_keyword_score(&crawl);
                let novelty_score = self.compute_novelty(&crawl);
                let structural_quality = self.compute_structural_quality(&crawl);
                let total_score = keyword_score * 0.4 + novelty_score * 0.3 + structural_quality * 0.3;

                RankedResult {
                    crawl,
                    semantic_score: 0.0,
                    keyword_score,
                    novelty_score,
                    structural_quality,
                    total_score,
                    embedding: None,
                }
            })
            .collect();

        ranked.sort_by(|a, b| b.total_score.partial_cmp(&a.total_score).unwrap());
        ranked.truncate(self.top_k);

        for r in &ranked {
            self.seen_hashes.push(content_hash(&r.crawl.content));
        }

        ranked
    }

    fn compute_keyword_score(&self, crawl: &CrawlResult) -> f32 {
        let text = format!("{} {}", crawl.title, crawl.content).to_lowercase();
        let mut score = 0.0f32;
        for keyword in &self.focus_keywords {
            score += text.matches(&keyword.to_lowercase()).count() as f32 * 0.5;
        }
        (score / 20.0).min(1.0)
    }

    fn compute_novelty(&self, crawl: &CrawlResult) -> f32 {
        let hash = content_hash(&crawl.content);
        if self.seen_hashes.contains(&hash) { 0.0 } else { 1.0 }
    }

    fn compute_structural_quality(&self, crawl: &CrawlResult) -> f32 {
        let mut score = 0.0f32;
        let len = crawl.content.len();
        if len > 200 { score += 0.3; }
        if len > 500 { score += 0.2; }
        if !crawl.title.is_empty() { score += 0.2; }
        if !crawl.links.is_empty() { score += 0.15; }
        if crawl.content.matches('.').count() > 3 { score += 0.15; }
        score.min(1.0)
    }
}

// ─────────────────────────────────────────────
// Fonctions mathématiques
// ─────────────────────────────────────────────

/// Similarité cosinus entre deux vecteurs
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    // Normaliser dans [0, 1]
    ((dot / (norm_a * norm_b)) + 1.0) / 2.0
}

fn content_hash(content: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hex::encode(hasher.finalize())
}
