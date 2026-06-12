//! # Batch Embeddings GPU
//!
//! Envoie N vecteurs d'un coup à /api/embed au lieu de 1 par 1.
//! L'API Ollama accepte un tableau `input` et retourne `embeddings[]`.
//! Gain x5-10 sur le ranking sémantique.

use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::{info, warn};

#[derive(Debug, Serialize)]
struct BatchEmbedRequest {
    model: String,
    input: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct BatchEmbedResponse {
    embeddings: Option<Vec<Vec<f32>>>,
}

/// Batch embedder — accumule les textes et envoie par batch
pub struct BatchEmbedder {
    client: Client,
    base_url: String,
    model: String,
    semaphore: Arc<Semaphore>,
    /// Taille max de batch (limité par VRAM)
    pub max_batch_size: usize,
    /// Statistiques
    pub total_embedded: u64,
    pub total_batches: u64,
}

impl BatchEmbedder {
    pub fn new(base_url: &str, model: &str, max_concurrent: usize, max_batch_size: usize) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.to_string(),
            model: model.to_string(),
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            max_batch_size,
            total_embedded: 0,
            total_batches: 0,
        }
    }

    /// Embedding batch : envoie tous les textes d'un coup
    /// Retourne Vec<Vec<f32>> aligné avec l'input
    pub async fn embed_batch(&mut self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let mut all_embeddings: Vec<Vec<f32>> = Vec::with_capacity(texts.len());

        // Découper en sous-batches si nécessaire
        for chunk in texts.chunks(self.max_batch_size) {
            let batch_result = self.embed_chunk(chunk).await?;
            all_embeddings.extend(batch_result);
        }

        self.total_embedded += texts.len() as u64;
        self.total_batches +=
            (texts.len() as u64 + self.max_batch_size as u64 - 1) / self.max_batch_size as u64;

        Ok(all_embeddings)
    }

    /// Envoyer un chunk de textes à Ollama
    async fn embed_chunk(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let _permit = self.semaphore.acquire().await?;

        let request = BatchEmbedRequest {
            model: self.model.clone(),
            input: texts.to_vec(),
        };

        let url = format!("{}/api/embed", self.base_url);
        let response = self.client.post(&url).json(&request).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Embed batch échoué: {} — {}", status, body));
        }

        let resp: BatchEmbedResponse = response.json().await?;

        match resp.embeddings {
            Some(embs) => {
                info!(
                    "Batch embed: {} textes → {} vecteurs",
                    texts.len(),
                    embs.len()
                );
                Ok(embs)
            }
            None => {
                warn!("Batch embed: pas d'embeddings retournés, fallback séquentiel");
                self.fallback_sequential(texts).await
            }
        }
    }

    /// Fallback séquentiel si le batch échoue
    async fn fallback_sequential(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let mut results = Vec::with_capacity(texts.len());

        for text in texts {
            let request = BatchEmbedRequest {
                model: self.model.clone(),
                input: vec![text.clone()],
            };

            let url = format!("{}/api/embed", self.base_url);
            match self.client.post(&url).json(&request).send().await {
                Ok(resp) => {
                    if let Ok(data) = resp.json::<BatchEmbedResponse>().await {
                        if let Some(mut embs) = data.embeddings {
                            if let Some(emb) = embs.pop() {
                                results.push(emb);
                                continue;
                            }
                        }
                    }
                    results.push(Vec::new());
                }
                Err(_) => results.push(Vec::new()),
            }
        }

        Ok(results)
    }

    /// Embedding d'un seul texte (pour compatibilité)
    pub async fn embed_single(&mut self, text: &str) -> Result<Vec<f32>> {
        let results = self.embed_batch(&[text.to_string()]).await?;
        results
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("Pas d'embedding"))
    }

    /// Similarité cosinus entre deux vecteurs
    pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() || a.is_empty() {
            return 0.0;
        }
        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        if na < 1e-10 || nb < 1e-10 {
            0.0
        } else {
            ((dot / (na * nb)) + 1.0) / 2.0
        }
    }

    /// Ranking sémantique batch : trier N textes par similarité avec une query
    pub async fn rank_by_similarity(
        &mut self,
        query: &str,
        texts: &[String],
    ) -> Result<Vec<(usize, f32)>> {
        // Embed query + tous les textes en un seul batch
        let mut all_texts = vec![query.to_string()];
        all_texts.extend_from_slice(texts);

        let embeddings = self.embed_batch(&all_texts).await?;
        if embeddings.len() < 2 {
            return Ok(Vec::new());
        }

        let query_emb = &embeddings[0];
        let mut scored: Vec<(usize, f32)> = embeddings[1..]
            .iter()
            .enumerate()
            .map(|(i, emb)| (i, Self::cosine_similarity(query_emb, emb)))
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        Ok(scored)
    }

    /// Stats d'utilisation
    pub fn stats(&self) -> BatchEmbedStats {
        BatchEmbedStats {
            total_embedded: self.total_embedded,
            total_batches: self.total_batches,
            avg_batch_size: if self.total_batches > 0 {
                self.total_embedded as f32 / self.total_batches as f32
            } else {
                0.0
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchEmbedStats {
    pub total_embedded: u64,
    pub total_batches: u64,
    pub avg_batch_size: f32,
}
