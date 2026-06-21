//! SoulSystem semantic-memory backend backed by [OctaSoma], a 3-D fractal
//! semantic memory engine.
//!
//! This is a thin SoulSystem-facing adapter over [`octasoma::OctaSomaAgent`]:
//! text is embedded, projected to 3-D, and indexed in a cache-line octree;
//! recall returns the topically-nearest stored memories.
//!
//! **Scope note (honest):** OctaSoma's 3-D projection is a *topical* router
//! (~70% cluster recall@1), not a high-recall ANN. Use it for coarse semantic
//! recall / preference & self-knowledge storage, or as a pre-filter ahead of a
//! full ANN — not as the sole exact-NN index.
//!
//! By default it uses the offline, deterministic [`octasoma::HashEmbedder`] so
//! it needs no model server; bring any [`octasoma::Embedder`] for real
//! semantics.

use octasoma::{EmbedError, Embedder, HashEmbedder, OctaSomaAgent};

/// A semantic memory backed by the OctaSoma fractal engine.
pub struct SemanticMemory<E: Embedder = HashEmbedder> {
    agent: OctaSomaAgent<E>,
}

impl SemanticMemory<HashEmbedder> {
    /// Offline, deterministic memory using a hash embedder of width `dim`.
    /// No model server required — ideal for tests and embedded use.
    pub fn offline(dim: usize, seed: u64) -> Self {
        Self {
            agent: OctaSomaAgent::new(HashEmbedder::new(dim), seed),
        }
    }

    /// Load an offline memory previously [`save`](Self::save)d.
    pub fn load_offline(dim: usize, path: &str) -> std::io::Result<Self> {
        Ok(Self {
            agent: OctaSomaAgent::from_file(HashEmbedder::new(dim), path)?,
        })
    }
}

impl<E: Embedder> SemanticMemory<E> {
    /// Build a memory over any embedder (e.g. an Ollama-backed one).
    pub fn with_embedder(embedder: E, seed: u64) -> Self {
        Self {
            agent: OctaSomaAgent::new(embedder, seed),
        }
    }

    /// Calibrate the 3-D projection (PCA) from a representative corpus, which
    /// markedly improves topical separation over the random default.
    pub fn calibrated(embedder: E, corpus: &[&str]) -> Result<Self, EmbedError> {
        Ok(Self {
            agent: OctaSomaAgent::calibrate(embedder, corpus)?,
        })
    }

    /// Store a memory (embed → project → index).
    pub fn remember(&mut self, text: &str) -> Result<(), EmbedError> {
        self.agent.perceive(text)
    }

    /// Recall the `k` topically-nearest memories, nearest first.
    pub fn recall(&self, query: &str, k: usize) -> Result<Vec<String>, EmbedError> {
        self.agent.recall(query, k)
    }

    /// Recall and join into a single prompt-ready context block.
    pub fn reflect(&self, query: &str, k: usize) -> Result<String, EmbedError> {
        self.agent.reflect(query, k)
    }

    /// Number of stored memories.
    pub fn len(&self) -> usize {
        self.agent.len()
    }

    /// Whether the memory is empty.
    pub fn is_empty(&self) -> bool {
        self.agent.is_empty()
    }

    /// Persist to a `.frac` file.
    pub fn save(&self, path: &str) -> std::io::Result<()> {
        self.agent.save(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remember_then_recall_is_topically_relevant() {
        let mut mem = SemanticMemory::offline(128, 42);
        mem.remember("Rust is a systems programming language")
            .unwrap();
        mem.remember("Python is great for quick prototyping")
            .unwrap();
        mem.remember("Octrees subdivide 3-D space efficiently")
            .unwrap();
        assert_eq!(mem.len(), 3);

        let hits = mem.recall("systems programming in Rust", 2).unwrap();
        assert_eq!(hits.len(), 2);
        // The Rust memory should surface for a Rust query.
        assert!(
            hits.iter().any(|h| h.contains("Rust")),
            "expected a Rust memory in recall, got {hits:?}"
        );
    }

    #[test]
    fn persistence_round_trips() {
        let dir = std::env::temp_dir();
        let path = dir
            .join(format!("soullink_octasoma_{}.frac", std::process::id()))
            .to_string_lossy()
            .into_owned();

        let mut mem = SemanticMemory::offline(64, 7);
        mem.remember("the user prefers metric units").unwrap();
        mem.remember("the project is written in Rust").unwrap();
        mem.save(&path).unwrap();

        let reloaded = SemanticMemory::load_offline(64, &path).unwrap();
        assert_eq!(reloaded.len(), 2);
        let hits = reloaded.recall("which units?", 1).unwrap();
        assert_eq!(hits.len(), 1);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn empty_memory_recall_is_empty() {
        let mem = SemanticMemory::offline(32, 1);
        assert!(mem.is_empty());
        assert!(mem.recall("anything", 5).unwrap().is_empty());
    }
}
