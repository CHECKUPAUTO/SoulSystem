//! MemoryHub — Hub memoire unifie avec events bus.
//!
//! Backend principal : soul-memory (sled/Qdrant, toujours actif).
//! Avec feature "full-memory" : soullink-memory (graphe conceptuel).

use anyhow::Result;
use soul_memory::SoulMemory;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;

#[cfg(feature = "full-memory")]
use soullink_memory::concept::{Concept, ConceptKind};

// Callback d'evenement memoire
pub type MemoryEventFn = Arc<dyn Fn(&str, serde_json::Value) + Send + Sync>;

// ── SimpleEmbedder ────────────────────────────────────────────────────

struct SimpleEmbedder { dim: usize, seeds: [u64; 8] }

impl SimpleEmbedder {
    fn new(dim: usize) -> Self {
        Self { dim, seeds: [42, 137, 251, 491, 773, 1021, 1301, 1607] }
    }
    fn embed(&self, text: &str) -> Vec<f32> {
        if text.is_empty() { return vec![0.0; self.dim]; }
        use std::hash::{Hash, Hasher};
        let chars: Vec<char> = text.chars().collect();
        let mut vec = vec![0.0f32; self.dim];
        for n in 2..=4usize {
            if n > chars.len() { continue; }
            for i in 0..=(chars.len() - n) {
                let ngram: String = chars[i..i + n].iter().collect();
                let mut h = std::collections::hash_map::DefaultHasher::new();
                ngram.hash(&mut h);
                let base = h.finish();
                let pw = 1.0 + (i as f32 / chars.len().max(1) as f32) * 0.5;
                for (slot, &seed) in self.seeds.iter().enumerate() {
                    let mut h2 = std::collections::hash_map::DefaultHasher::new();
                    (base, seed).hash(&mut h2);
                    let idx = h2.finish() as usize % self.dim;
                    vec[idx] += if (slot & 1) == 0 { 1.0 } else { -1.0 } * pw;
                }
            }
        }
        let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 { for x in &mut vec { *x /= norm; } }
        vec
    }
}

// ── MemoryHub ──────────────────────────────────────────────────────────

pub struct MemoryHub {
    pub vector: SoulMemory,
    pub on_event: Option<MemoryEventFn>,
}

impl MemoryHub {
    pub async fn new(data_dir: &std::path::Path) -> Self {
        let _ = data_dir;
        let vector = SoulMemory::new().expect("SoulMemory doit s'initialiser");
        info!("MemoryHub: soul-memory actif");
        Self { vector, on_event: None }
    }

    pub fn set_event_callback(&mut self, cb: MemoryEventFn) {
        self.on_event = Some(cb);
    }

    fn emit(&self, kind: &str, payload: serde_json::Value) {
        if let Some(ref cb) = self.on_event { cb(kind, payload); }
    }

    pub async fn store(&self, text: &str, metadata: HashMap<String, String>) -> Result<()> {
        self.vector.store(text, metadata.clone()).await?;
        self.emit("stored", serde_json::json!({"text": text, "metadata": metadata}));
        Ok(())
    }

    pub async fn search(&self, query: &str, top_k: usize) -> Vec<SearchResult> {
        let mut results: Vec<SearchResult> = Vec::new();
        if let Ok(hits) = self.vector.search(query, top_k).await {
            for (text, score) in hits {
                results.push(SearchResult { text, score, source: "vector" });
            }
        }
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        results.dedup_by(|a, b| a.text == b.text);
        results.truncate(top_k);
        results
    }

    pub async fn get_context(&self, query: &str, limit: usize) -> String {
        let results = self.search(query, limit).await;
        if results.is_empty() { return String::new(); }
        let mut ctx = String::from("Contexte memoire:\n");
        for (i, r) in results.iter().enumerate() {
            ctx.push_str(&format!("[{}] ({}) {}\n", i, r.source, r.text));
        }
        ctx
    }

    pub async fn decay_and_prune(&self, threshold: f32, decay: f32, max: usize) {
        let _ = self.vector.decay_and_prune(threshold, decay, max);
        self.emit("decayed", serde_json::json!({"threshold": threshold, "max": max}));
    }
}

#[derive(Debug, Clone)]
pub struct SearchResult { pub text: String, pub score: f32, pub source: &'static str }

// ── Tests ──────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    #[tokio::test]
    async fn test_store_and_search() {
        let dir = tempfile::TempDir::new().unwrap();
        let hub = MemoryHub::new(dir.path()).await;
        hub.store("Le chat noir dort", HashMap::new()).await.unwrap();
        assert!(!hub.search("chat noir", 5).await.is_empty());
    }
    #[tokio::test]
    async fn test_context() {
        let dir = tempfile::TempDir::new().unwrap();
        let hub = MemoryHub::new(dir.path()).await;
        hub.store("document de test", HashMap::new()).await.unwrap();
        assert!(hub.get_context("document", 5).await.contains("document de test"));
    }
    #[tokio::test]
    async fn test_event_callback() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut hub = MemoryHub::new(dir.path()).await;
        let fired = Arc::new(AtomicBool::new(false));
        let f = fired.clone();
        hub.set_event_callback(Arc::new(move |kind, _| {
            if kind == "stored" { f.store(true, Ordering::SeqCst); }
        }));
        hub.store("test", HashMap::new()).await.unwrap();
        assert!(fired.load(Ordering::SeqCst));
    }
}
