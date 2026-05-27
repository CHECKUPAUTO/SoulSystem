//! MemoryHub — Hub mémoire unifié.
//!
//! Point d'entrée unique pour toutes les opérations mémoire.
//! Backend principal : soul-memory (sled/Qdrant).
//! Les crates soullink-memory et soullink-rag pourront être ajoutés
//! ultérieurement via la feature "full-memory".

use anyhow::Result;
use soul_memory::SoulMemory;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;

// ── MemoryHub ──────────────────────────────────────────────────────────

/// Hub mémoire unifié. Actuellement basé sur soul-memory (vectoriel).
pub struct MemoryHub {
    pub vector: SoulMemory,
}

impl MemoryHub {
    /// Crée le hub mémoire.
    pub async fn new(data_dir: &std::path::Path) -> Self {
        let vector = SoulMemory::new().expect("SoulMemory doit s'initialiser");
        info!("MemoryHub: soul-memory actif (data_dir: {:?})", data_dir);
        Self { vector }
    }

    /// Stocke un texte avec métadonnées dans le stockage vectoriel.
    pub async fn store(&self, text: &str, metadata: HashMap<String, String>) -> Result<()> {
        self.vector.store(text, metadata).await?;
        Ok(())
    }

    /// Recherche les textes les plus proches d'une requête.
    pub async fn search(&self, query: &str, top_k: usize) -> Vec<SearchResult> {
        match self.vector.search(query, top_k).await {
            Ok(results) => results.into_iter().map(|(text, score)| SearchResult {
                text, score, source: "soul-memory",
            }).collect(),
            Err(e) => {
                tracing::warn!("MemoryHub search error: {}", e);
                Vec::new()
            }
        }
    }

    /// Formatte le contexte pour injection dans un prompt.
    pub async fn get_context(&self, query: &str, limit: usize) -> String {
        let results = self.search(query, limit).await;
        if results.is_empty() { return String::new(); }
        let mut ctx = String::from("Contexte memoire:\n");
        for (i, r) in results.iter().enumerate() {
            ctx.push_str(&format!("[{}] ({}) {}\n", i, r.source, r.text));
        }
        ctx
    }

    /// Décroissance temporelle + nettoyage.
    pub async fn decay_and_prune(&self, threshold: f32, decay_factor: f32, max_entries: usize) {
        let _ = self.vector.decay_and_prune(threshold, decay_factor, max_entries);
    }
}

/// Résultat de recherche unifié.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub text: String,
    pub score: f32,
    pub source: &'static str,
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_hub_store_and_search() {
        let dir = tempfile::TempDir::new().unwrap();
        let hub = MemoryHub::new(dir.path()).await;
        let mut meta = HashMap::new();
        meta.insert("source".into(), "test".into());
        hub.store("Le chat noir dort sur le canape", meta).await.unwrap();
        let results = hub.search("chat canape", 5).await;
        assert!(!results.is_empty(), "devrait trouver le texte stocke");
    }
}
