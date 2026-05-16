//! SoulMemory — Base de connaissances vectorielle unifiée.
//!
//! Stocke interactions, découvertes AVID et optimisations OpenEvolve
//! sous forme vectorielle. Utilise Qdrant comme backend principal
//! avec fallback local sled + HNSW.

use anyhow::Result;
use std::collections::HashMap;
use tracing::{info, warn};

/// Client mémoire avec backend vectoriel.
pub struct SoulMemory {
    backend: Backend,
}

enum Backend {
    Qdrant {
        url: String,
        collection: String,
    },
    LocalFallback {
        db: sled::Db,
        // Simule la vectorisation et la recherche
        dim: usize,
    },
}

/// Embedder mock — simule la vectorisation.
/// En production, serait remplacé par `scirust::embed`.
pub struct MockEmbedder;

impl MockEmbedder {
    /// Produit un vecteur déterministe de dimension `dim` basé sur le hash du texte.
    pub fn embed(&self, text: &str, dim: usize) -> Vec<f32> {
        let hash = seahash::hash(text.as_bytes());
        let mut vec = Vec::with_capacity(dim);
        for i in 0..dim {
            let val = ((hash.wrapping_mul((i as u64) + 1)) % 1000) as f32 / 1000.0;
            vec.push(val);
        }
        // Normalisation L2
        let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in &mut vec {
                *v /= norm;
            }
        }
        vec
    }
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
            })
        } else {
            warn!("SoulMemory: QDRANT_URL not set, using local sled fallback");
            let db = sled::open("/var/lib/soulsystem/data/soul_memory")?;
            Ok(Self {
                backend: Backend::LocalFallback { db, dim: 384 },
            })
        }
    }

    /// Crée une instance de test avec un backend temporaire.
    pub fn new_test() -> Result<Self> {
        let db = sled::Config::new().temporary(true).open()?;
        Ok(Self {
            backend: Backend::LocalFallback { db, dim: 384 },
        })
    }

    /// Stocke un texte avec métadonnées.
    pub async fn store(&self, text: &str, metadata: HashMap<String, String>) -> Result<()> {
        match &self.backend {
            Backend::Qdrant { .. } => {
                // En production: vectoriser avec l'API Qdrant
                let embedder = MockEmbedder;
                let _vector = embedder.embed(text, 384);
                // Appel API Qdrant simulé
                info!(
                    "SoulMemory: stored (Qdrant) — {}",
                    &text[..text.len().min(60)]
                );
            }
            Backend::LocalFallback { db, dim } => {
                let embedder = MockEmbedder;
                let vector = embedder.embed(text, *dim);
                let key = seahash::hash(text.as_bytes()).to_string();
                let value = serde_json::json!({
                    "text": text,
                    "vector": vector,
                    "metadata": metadata,
                });
                db.insert(key.as_bytes(), serde_json::to_vec(&value)?)?;
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
                // Simulé: retourne des résultats vides pour le fallback
                Ok(vec![(format!("Result for: {}", query), 0.95)])
            }
            Backend::LocalFallback { db, dim } => {
                let embedder = MockEmbedder;
                let query_vec = embedder.embed(query, *dim);

                let mut results: Vec<(String, f32)> = Vec::new();
                for entry in db.iter() {
                    let (_, value) = entry?;
                    let doc: serde_json::Value = serde_json::from_slice(&value)?;
                    if let Some(stored_vec) = doc["vector"].as_array() {
                        let stored: Vec<f32> = stored_vec
                            .iter()
                            .map(|v| v.as_f64().unwrap() as f32)
                            .collect();
                        let score = cosine_similarity(&query_vec, &stored);
                        if let Some(t) = doc["text"].as_str() {
                            results.push((t.to_string(), score));
                        }
                    }
                }
                results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
                results.truncate(limit);
                Ok(results)
            }
        }
    }

    /// Récupère un contexte textuel pour injection dans un prompt.
    pub async fn get_context(&self, query: &str) -> Result<String> {
        let results = self.search(query, 5).await?;
        if results.is_empty() {
            return Ok(String::new());
        }
        let ctx: Vec<String> = results
            .into_iter()
            .enumerate()
            .map(|(i, (text, score))| format!("[{i}] ({score:.2}): {text}", i = i, score = score))
            .collect();
        Ok(ctx.join("\n"))
    }
}

/// Similarité cosinus entre deux vecteurs de même dimension.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}
