//! MemoryHub — Hub memoire unifie avec graphe conceptuel (full-memory).
//!
//! Backend principal : soul-memory (sled/Qdrant, toujours actif).
//! Avec feature "full-memory" (activee par defaut) : ajoute le graphe
//! conceptuel soullink-memory pour la navigation associative.

use anyhow::Result;
use soul_memory::SoulMemory;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

// Imports conditionnels full-memory
#[cfg(feature = "full-memory")]
use soullink_memory::graph::MemoryGraph;
#[cfg(feature = "full-memory")]
use soullink_memory::concept::{Concept, ConceptKind};
#[cfg(feature = "full-memory")]
use soullink_memory::DecayConfig;

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
    #[cfg(feature = "full-memory")]
    pub graph: Option<Arc<RwLock<MemoryGraph>>>,
    pub on_event: Option<MemoryEventFn>,
}

impl MemoryHub {
    pub async fn new(data_dir: &std::path::Path) -> Self {
        let _ = data_dir;
        let vector = SoulMemory::new().expect("SoulMemory doit s'initialiser");
        info!("MemoryHub: soul-memory actif");

        #[cfg(feature = "full-memory")]
        let graph = match MemoryGraph::open(
            &data_dir.join("concept_graph"),
            DecayConfig::default(),
        ) {
            Ok(g) => {
                info!("MemoryHub: graphe conceptuel actif (soullink-memory)");
                Some(Arc::new(RwLock::new(g)))
            }
            Err(e) => {
                tracing::warn!("MemoryHub: graphe conceptuel indisponible: {}", e);
                None
            }
        };

        Self { vector, graph, on_event: None }
    }

    pub fn set_event_callback(&mut self, cb: MemoryEventFn) {
        self.on_event = Some(cb);
    }

    fn emit(&self, kind: &str, payload: serde_json::Value) {
        if let Some(ref cb) = self.on_event { cb(kind, payload); }
    }

    pub async fn store(&self, text: &str, metadata: HashMap<String, String>) -> Result<()> {
        // Toujours stocker dans soul-memory (vectoriel)
        self.vector.store(text, metadata.clone()).await?;

        // Si full-memory : stocker aussi dans le graphe conceptuel
        #[cfg(feature = "full-memory")]
        if let Some(ref graph) = self.graph {
            let kind = classify_text(text, &metadata);
            let _ = graph.write().await.insert(Concept::new(text, kind), None);
        }

        self.emit("stored", serde_json::json!({"text": text, "metadata": metadata}));
        Ok(())
    }

    pub async fn search(&self, query: &str, top_k: usize) -> Vec<SearchResult> {
        let mut results: Vec<SearchResult> = Vec::new();

        // 1. SoulMemory (vectoriel)
        if let Ok(hits) = self.vector.search(query, top_k).await {
            for (text, score) in hits {
                results.push(SearchResult { text, score, source: "vector" });
            }
        }

        // 2. Graphe conceptuel (full-memory)
        #[cfg(feature = "full-memory")]
        if let Some(ref graph) = self.graph {
            let qv = SimpleEmbedder::new(64).embed(query);
            for r in &graph.read().await.search(&qv, top_k) {
                results.push(SearchResult { text: r.label.clone(), score: r.score, source: "graph" });
            }
        }

        // Tri + dedup
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
        #[cfg(feature = "full-memory")]
        if let Some(ref graph) = self.graph {
            let _ = graph.write().await.decay_all();
        }
        self.emit("decayed", serde_json::json!({"threshold": threshold, "max": max}));
    }
}

#[derive(Debug, Clone)]
pub struct SearchResult { pub text: String, pub score: f32, pub source: &'static str }

#[cfg(feature = "full-memory")]
fn classify_text(_text: &str, meta: &HashMap<String, String>) -> ConceptKind {
    if let Some(tag) = meta.get("tag") {
        match tag.as_str() {
            "skill" | "competence" => return ConceptKind::Skill,
            "project" => return ConceptKind::Project,
            "person" | "user" => return ConceptKind::Person,
            _ => {}
        }
    }
    if let Some(source) = meta.get("source") {
        match source.as_str() {
            "audit" | "security" => return ConceptKind::Event,
            _ => {}
        }
    }
    ConceptKind::Fact
}

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
