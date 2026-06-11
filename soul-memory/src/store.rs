use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::{info, warn};

use crate::{cosine_similarity, compute_initial_importance, Embedder, NGramEmbedder, SciRustEmbedder};

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MemoryEntry {
    id: u64,
    text: String,
    vector: Vec<f32>,
    metadata: HashMap<String, String>,
    #[serde(default = "default_importance")]
    importance: f32,
}

fn default_importance() -> f32 {
    1.0
}

enum Backend {
    Qdrant { url: String, collection: String },
    LocalFallback { db: sled::Db, _dim: usize },
}

pub struct SoulMemory {
    backend: Backend,
    embedder: Box<dyn Embedder>,
    next_id: AtomicU64,
}

impl SoulMemory {
    pub fn new() -> Result<Self> {
        Self::with_embedder(Box::new(SciRustEmbedder::new(64)))
    }

    pub fn with_embedder(embedder: Box<dyn Embedder>) -> Result<Self> {
        let dim = embedder.dim();
        let qdrant_url = std::env::var("QDRANT_URL").unwrap_or_default();
        if !qdrant_url.is_empty() {
            info!("SoulMemory: using Qdrant at {}", qdrant_url);
            Ok(Self {
                backend: Backend::Qdrant { url: qdrant_url, collection: "soul_memory".into() },
                embedder,
                next_id: AtomicU64::new(1),
            })
        } else {
            warn!("SoulMemory: QDRANT_URL not set, using local sled fallback");
            let db = sled::Config::new().temporary(true).open()?;
            let max_id = Self::load_max_id(&db).unwrap_or(1);
            Ok(Self {
                backend: Backend::LocalFallback { db, _dim: dim },
                embedder,
                next_id: AtomicU64::new(max_id),
            })
        }
    }

    pub fn new_test() -> Result<Self> {
        Self::new_test_with_embedder(Box::new(SciRustEmbedder::new(64)))
    }

    pub fn new_test_with_embedder(embedder: Box<dyn Embedder>) -> Result<Self> {
        let dim = embedder.dim();
        let db = sled::Config::new().temporary(true).open()?;
        Ok(Self {
            backend: Backend::LocalFallback { db, _dim: dim },
            embedder,
            next_id: AtomicU64::new(1),
        })
    }

    fn load_max_id(db: &sled::Db) -> Option<u64> {
        let mut max = 0u64;
        for (key, _) in db.iter().flatten() {
            if let Ok(key_str) = std::str::from_utf8(&key) {
                if let Ok(id) = key_str.parse::<u64>() {
                    if id > max { max = id; }
                }
            }
        }
        if max > 0 { Some(max + 1) } else { None }
    }

    pub async fn store(&self, text: &str, metadata: HashMap<String, String>) -> Result<()> {
        let importance = compute_initial_importance(text, &metadata);
        self.store_with_importance(text, metadata, importance).await
    }

    pub async fn store_with_importance(
        &self, text: &str, metadata: HashMap<String, String>, importance: f32,
    ) -> Result<()> {
        match &self.backend {
            Backend::Qdrant { url, collection } => {
                let vector = self.embedder.embed(text);
                let _ = (vector, url, collection);
                info!("SoulMemory: stored (Qdrant mock, importance={:.2}) — {}", importance, &text[..text.len().min(60)]);
            }
            Backend::LocalFallback { db, _dim: _ } => {
                let vector = self.embedder.embed(text);
                let entry = MemoryEntry {
                    id: self.next_id.load(Ordering::Relaxed),
                    text: text.to_string(),
                    vector,
                    metadata,
                    importance,
                };
                let key = entry.id.to_string();
                db.insert(key.as_bytes(), serde_json::to_vec(&entry)?)?;
                self.next_id.fetch_add(1, Ordering::Relaxed);
                info!("SoulMemory: stored (local, importance={:.2}) — {}", importance, &text[..text.len().min(60)]);
            }
        }
        Ok(())
    }

    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<(String, f32)>> {
        match &self.backend {
            Backend::Qdrant { .. } => {
                let query_vec = self.embedder.embed(query);
                Ok(vec![(
                    format!("Search Qdrant for: {}", query),
                    cosine_similarity(&query_vec, &query_vec),
                )])
            }
            Backend::LocalFallback { db, _dim: _ } => {
                let query_vec = self.embedder.embed(query);
                let mut results: Vec<(String, f32)> = Vec::new();
                for entry in db.iter() {
                    let (_, value) = entry?;
                    let entry: MemoryEntry = serde_json::from_slice(&value)?;
                    let score = cosine_similarity(&query_vec, &entry.vector);
                    results.push((entry.text, score));
                }
                results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                results.truncate(limit);
                Ok(results)
            }
        }
    }

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

    pub fn count(&self) -> Result<usize> {
        match &self.backend {
            Backend::Qdrant { .. } => Ok(0),
            Backend::LocalFallback { db, .. } => Ok(db.iter().count()),
        }
    }

    pub fn decay_and_prune(&self, decay_factor: f32, threshold: f32, max_entries: usize) -> Result<(usize, usize)> {
        match &self.backend {
            Backend::Qdrant { .. } => {
                info!("SoulMemory: decay_and_prune skipped (Qdrant backend)");
                Ok((0, 0))
            }
            Backend::LocalFallback { db, .. } => {
                let mut entries: Vec<(u64, MemoryEntry)> = Vec::new();
                for item in db.iter() {
                    let (key, value) = item?;
                    let mut entry: MemoryEntry = serde_json::from_slice(&value)?;
                    entry.importance *= decay_factor;
                    entries.push((
                        std::str::from_utf8(&key).unwrap_or("0").parse().unwrap_or(0),
                        entry,
                    ));
                }

                let (mut keep, remove): (Vec<_>, Vec<_>) = entries
                    .into_iter()
                    .partition(|(_, e)| e.importance >= threshold);
                let mut removed = 0usize;
                for (id, _) in &remove {
                    db.remove(id.to_string().as_bytes())?;
                    removed += 1;
                }
                if keep.len() > max_entries {
                    keep.sort_by(|a, b| {
                        a.1.importance
                            .partial_cmp(&b.1.importance)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                    let to_remove = keep.len() - max_entries;
                    for (id, _) in keep.iter().take(to_remove) {
                        db.remove(id.to_string().as_bytes())?;
                        removed += 1;
                    }
                    keep.drain(0..to_remove);
                }
                for (id, entry) in &keep {
                    db.insert(id.to_string().as_bytes(), serde_json::to_vec(entry)?)?;
                }
                info!("SoulMemory: decay_and_prune — {} kept, {} removed", keep.len(), removed);
                Ok((keep.len(), removed))
            }
        }
    }

    pub fn embedder(&self) -> &dyn Embedder {
        self.embedder.as_ref()
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
        mem.store("Le chat noir dort sur le canape", meta1).await.unwrap();
        let mut meta2 = HashMap::new();
        meta2.insert("source".into(), "test2".into());
        mem.store("Le chien brun court dans le jardin", meta2).await.unwrap();
        let results = mem.search("chat noir canape", 2).await.unwrap();
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
        mem.store("Premier document de test", HashMap::new()).await.unwrap();
        let ctx = mem.get_context("document test").await.unwrap();
        assert!(!ctx.is_empty());
        assert!(ctx.contains("[0]"));
    }

    #[tokio::test]
    async fn test_decay_and_prune_removes_weak() {
        let mem = SoulMemory::new_test().unwrap();
        for i in 0..5 {
            let mut meta = HashMap::new();
            meta.insert("n".into(), i.to_string());
            mem.store_with_importance(&format!("document numero {}", i), meta, 0.5).await.unwrap();
        }
        assert_eq!(mem.count().unwrap(), 5);
        let (kept, removed) = mem.decay_and_prune(0.1, 0.1, 100).unwrap();
        assert_eq!(removed, 5);
        assert_eq!(kept, 0);
    }

    #[tokio::test]
    async fn test_embedder_injection() {
        let mem = SoulMemory::new_test_with_embedder(Box::new(NGramEmbedder::new(128))).unwrap();
        let mut meta = HashMap::new();
        meta.insert("source".into(), "injection_test".into());
        mem.store("Test avec NGramEmbedder", meta).await.unwrap();
        let results = mem.search("NGramEmbedder", 1).await.unwrap();
        assert_eq!(results.len(), 1);
    }
}