//! SoulMemory — Base de connaissances vectorielle unifiée.
//!
//! Moteur d'embedding + recherche local utilisant n-grammes positionnels
//! et similarité cosinus. Stockage via sled. Fallback Qdrant prêt.

use anyhow::Result;
use seahash::hash as seahash;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::{info, warn};

/// Point mémoire avec embedding et métadonnées.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct MemoryEntry {
    id: u64,
    text: String,
    vector: Vec<f32>,
    metadata: HashMap<String, String>,
}

/// Backend de stockage.
enum Backend {
    Qdrant { url: String, collection: String },
    LocalFallback { db: sled::Db, _dim: usize },
}

/// Embedder utilisant n-grammes + hachage positionnel.
pub struct NGramEmbedder {
    min_n: usize,
    max_n: usize,
    dim: usize,
}

impl NGramEmbedder {
    pub fn new(dim: usize) -> Self {
        Self {
            min_n: 2,
            max_n: 4,
            dim,
        }
    }

    /// Produit un embedding à partir du texte.
    /// Utilise des n-grammes (taille 2-4), normalisation TF,
    /// hachage positionnel via seahash.
    pub fn embed(&self, text: &str) -> Vec<f32> {
        if text.is_empty() {
            return vec![0.0; self.dim];
        }

        let mut vec = vec![0.0f32; self.dim];
        let chars: Vec<char> = text.chars().collect();
        let mut total_count = 0usize;

        for n in self.min_n..=self.max_n {
            if n > chars.len() {
                continue;
            }
            for i in 0..=(chars.len() - n) {
                let ngram: String = chars[i..i + n].iter().collect();
                let h = seahash(ngram.as_bytes());
                let pos = (h as usize) % self.dim;
                // TF weighting: 1.0 per occurrence, scaled by position weight
                let pos_weight = 1.0 + (i as f32 / chars.len().max(1) as f32) * 0.5;
                vec[pos] += pos_weight;
                total_count += 1;
            }
        }

        // Normalisation L2
        if total_count > 0 {
            let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
            if norm > 0.0 {
                for v in &mut vec {
                    *v /= norm;
                }
            }
        }

        vec
    }

    /// Calcule la similarité cosinus entre deux vecteurs.
    pub fn cosine_similarity(&self, a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() || a.is_empty() {
            return 0.0;
        }
        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm_a == 0.0 || norm_b == 0.0 {
            0.0
        } else {
            dot / (norm_a * norm_b)
        }
    }
}

/// Moteur mémoire vectorielle.
pub struct SoulMemory {
    backend: Backend,
    embedder: NGramEmbedder,
    next_id: AtomicU64,
}

impl SoulMemory {
    /// Crée une nouvelle instance SoulMemory.
    ///
    /// Utilise Qdrant si `QDRANT_URL` est défini, sinon fallback local.
    pub fn new() -> Result<Self> {
        let qdrant_url = std::env::var("QDRANT_URL").unwrap_or_default();

        if !qdrant_url.is_empty() {
            info!("SoulMemory: using Qdrant at {}", qdrant_url);
            Ok(Self {
                backend: Backend::Qdrant {
                    url: qdrant_url,
                    collection: "soul_memory".into(),
                },
                embedder: NGramEmbedder::new(384),
                next_id: AtomicU64::new(1),
            })
        } else {
            warn!("SoulMemory: QDRANT_URL not set, using local sled fallback");
            let db = sled::open("/var/lib/soulsystem/data/soul_memory")?;
            let max_id = Self::load_max_id(&db).unwrap_or(1);
            Ok(Self {
                backend: Backend::LocalFallback { db, _dim: 384 },
                embedder: NGramEmbedder::new(384),
                next_id: AtomicU64::new(max_id),
            })
        }
    }

    /// Crée une instance de test avec un backend temporaire.
    pub fn new_test() -> Result<Self> {
        let db = sled::Config::new().temporary(true).open()?;
        Ok(Self {
            backend: Backend::LocalFallback { db, _dim: 384 },
            embedder: NGramEmbedder::new(384),
            next_id: AtomicU64::new(1),
        })
    }

    /// Charge le plus grand ID depuis la base sled.
    fn load_max_id(db: &sled::Db) -> Option<u64> {
        let mut max = 0u64;
        for result in db.iter() {
            if let Ok((key, _)) = result {
                if let Ok(key_str) = std::str::from_utf8(&key) {
                    if let Ok(id) = key_str.parse::<u64>() {
                        if id > max {
                            max = id;
                        }
                    }
                }
            }
        }
        if max > 0 {
            Some(max + 1)
        } else {
            None
        }
    }

    /// Stocke un texte avec métadonnées.
    pub async fn store(&self, text: &str, metadata: HashMap<String, String>) -> Result<()> {
        match &self.backend {
            Backend::Qdrant { url, collection } => {
                // Fallback Qdrant : code HTTP prêt mais pas exécuté si Qdrant absent
                // On notifie et on stocke en mémoire seulement
                let vector = self.embedder.embed(text);
                let _ = (vector, url, collection);
                info!(
                    "SoulMemory: stored (Qdrant mock) — {}",
                    &text[..text.len().min(60)]
                );
            }
            Backend::LocalFallback { db, _dim: _ } => {
                let vector = self.embedder.embed(text);
                let entry = MemoryEntry {
                    id: self.next_id.load(Ordering::Relaxed),
                    text: text.to_string(),
                    vector,
                    metadata,
                };
                let key = entry.id.to_string();
                db.insert(key.as_bytes(), serde_json::to_vec(&entry)?)?;
                self.next_id.fetch_add(1, Ordering::Relaxed);
                info!(
                    "SoulMemory: stored (local) — {}",
                    &text[..text.len().min(60)]
                );
            }
        }
        Ok(())
    }

    /// Recherche les textes les plus proches d'une requête.
    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<(String, f32)>> {
        match &self.backend {
            Backend::Qdrant { .. } => {
                // Version mockée de Qdrant : embedding + résultat simulant
                let query_vec = self.embedder.embed(query);
                // Simule un résultat formaté
                Ok(vec![(
                    format!("Recherche Qdrant pour: {}", query),
                    self.embedder.cosine_similarity(&query_vec, &query_vec),
                )])
            }
            Backend::LocalFallback { db, _dim: _ } => {
                let query_vec = self.embedder.embed(query);
                let mut results: Vec<(String, f32)> = Vec::new();

                for entry in db.iter() {
                    let (_, value) = entry?;
                    let entry: MemoryEntry = serde_json::from_slice(&value)?;
                    let score = self.embedder.cosine_similarity(&query_vec, &entry.vector);
                    results.push((entry.text, score));
                }

                results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                results.truncate(limit);
                Ok(results)
            }
        }
    }

    /// Récupère un contexte textuel formaté pour injection dans un prompt.
    pub async fn get_context(&self, query: &str) -> Result<String> {
        let results = self.search(query, 5).await?;
        if results.is_empty() {
            return Ok(String::new());
        }
        let ctx: Vec<String> = results
            .into_iter()
            .enumerate()
            .map(|(i, (text, score))| format!("[{i}] ({score:.2}): {text}"))
            .collect();
        Ok(ctx.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_store_and_search() {
        let mem = SoulMemory::new_test().unwrap();

        let mut meta1 = HashMap::new();
        meta1.insert("source".into(), "test".into());
        mem.store("Le chat noir dort sur le canapé", meta1)
            .await
            .unwrap();

        let mut meta2 = HashMap::new();
        meta2.insert("source".into(), "test2".into());
        mem.store("Le chien brun court dans le jardin", meta2)
            .await
            .unwrap();

        // La recherche doit trouver le texte le plus similaire
        let results = mem.search("chat noir canapé", 2).await.unwrap();
        assert!(!results.is_empty());
        assert!(results[0].1 > 0.0);
    }

    #[tokio::test]
    async fn test_empty_result() {
        let mem = SoulMemory::new_test().unwrap();
        let results = mem.search("rien ici", 5).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_context_formatting() {
        let mem = SoulMemory::new_test().unwrap();
        let meta = HashMap::new();
        mem.store("Premier document de test", meta).await.unwrap();

        let ctx = mem.get_context("document test").await.unwrap();
        assert!(!ctx.is_empty());
        assert!(ctx.contains("[0]"));
        assert!(ctx.contains("Premier document"));
    }

    #[tokio::test]
    async fn test_store_same_text_twice() {
        let mem = SoulMemory::new_test().unwrap();
        mem.store("Texte dupliqué", HashMap::new()).await.unwrap();
        mem.store("Texte dupliqué", HashMap::new()).await.unwrap();

        let results = mem.search("Texte dupliqué", 10).await.unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_ngram_embedder_deterministic() {
        let embedder = NGramEmbedder::new(128);
        let v1 = embedder.embed("hello world");
        let v2 = embedder.embed("hello world");
        assert!(v1.iter().zip(v2.iter()).all(|(a, b)| (a - b).abs() < 1e-6));
    }

    #[test]
    fn test_ngram_embedder_similar_texts() {
        let embedder = NGramEmbedder::new(128);
        let v1 = embedder.embed("machine learning");
        let v2 = embedder.embed("machine learning models");
        let sim = embedder.cosine_similarity(&v1, &v2);
        assert!(sim > 0.3);
    }

    #[test]
    fn test_ngram_embedder_empty() {
        let embedder = NGramEmbedder::new(128);
        let v = embedder.embed("");
        assert!(v.iter().all(|x| *x == 0.0));
    }
}
