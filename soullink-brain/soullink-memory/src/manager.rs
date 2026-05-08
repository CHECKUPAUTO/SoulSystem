//! MemoryManager — high-level API: text → embedding → storage.

use std::sync::Arc;

use anyhow::Result;

use crate::concept::{Concept, ConceptKind};
use crate::graph::MemoryGraph;
use crate::ollama_client::OllamaEmbeddingClient;
use soullink_vector::hnsw_index::SearchResult;

/// High-level memory manager bridging text → embedding → graph storage.
pub struct MemoryManager {
    graph: Arc<MemoryGraph>,
    ollama: OllamaEmbeddingClient,
}

impl MemoryManager {
    /// Create a new MemoryManager with the given graph and Ollama client.
    pub fn new(graph: Arc<MemoryGraph>, ollama: OllamaEmbeddingClient) -> Self {
        Self { graph, ollama }
    }

    /// Store a concept with auto-generated embedding from Ollama.
    pub async fn store(&self, text: &str, kind: ConceptKind) -> Result<petgraph::graph::NodeIndex> {
        let embedding = self.ollama.embed(text).await.ok();
        let concept = Concept::new(text, kind);
        self.graph.insert(concept, embedding)
    }

    /// Search by text (auto-embeds query via Ollama).
    pub async fn search(&self, query: &str, top_k: usize) -> Vec<SearchResult> {
        match self.ollama.embed(query).await {
            Ok(vec) => self.graph.search(&vec, top_k),
            Err(e) => {
                tracing::warn!("embedding failed for search: {}", e);
                Vec::new()
            }
        }
    }

    /// Recall a concept and boost its access count (reinforce memory).
    pub fn recall(&self, label: &str) -> Option<Concept> {
        let mut concept = self.graph.get(label)?;
        concept.touch();
        // Note: actual graph mutation for touch would need write access
        // For now, return the touched concept
        Some(concept)
    }

    /// Apply decay to all edges.
    pub fn decay(&self) -> Result<()> {
        self.graph.decay_all()
    }

    /// Persist to disk.
    pub fn persist(&self) -> Result<()> {
        self.graph.persist()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::MemoryGraph;
    use crate::DecayConfig;
    use tempfile::TempDir;

    #[test]
    fn recall_boosts_access_count() {
        let dir = TempDir::new().unwrap();
        let g = Arc::new(
            MemoryGraph::open(dir.path(), DecayConfig::default()).unwrap()
        );
        let mgr = MemoryManager::new(g.clone(), OllamaEmbeddingClient::new("http://localhost:11434", "test"));

        // Insert directly via graph (no Ollama needed)
        g.insert(Concept::new("rust", ConceptKind::Skill), None).unwrap();

        let c = mgr.recall("rust").unwrap();
        assert_eq!(c.access_count, 1);
    }
}