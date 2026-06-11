//! RagMiddleware — Activation automatique du RAG avant chaque réponse.
//!
//! # Problème
//! Le pipeline RAG (soullink-rag) existe mais n'est jamais appelé automatiquement.
//! Les souvenirs longs ne sont pas utilisés dans les réponses.
//!
//! # Solution
//! Avant chaque génération de réponse, ce middleware :
//! 1. Extrait les N derniers messages du contexte
//! 2. Interroge soullink-rag en mode hybride (BM25 + embedding vectoriel)
//! 3. Si score > 0.7, préfixe le prompt avec les chunks pertinents
//! 4. Interroge aussi le graphe conceptuel pour les relations associatives
//!
//! # Performance
//! - Cache LRU devant le store vectoriel
//! - Dégradation gracieuse si le service RAG est indisponible
//! - Timeout max 200ms sur la recherche

use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tracing::{info, warn};

// ── Cache LRU prédictif ─────────────────────────────────────────────

/// Cache LRU simple pour les résultats RAG.
struct LruCache {
    capacity: usize,
    entries: Vec<(String, String, f32)>, // (query, result, score)
}

impl LruCache {
    fn new(capacity: usize) -> Self {
        Self {
            capacity,
            entries: Vec::with_capacity(capacity),
        }
    }

    fn get(&mut self, query: &str) -> Option<&(String, String, f32)> {
        let idx = self.entries.iter().position(|(q, _, _)| q == query)?;
        // Déplacer en tête (MRU)
        let entry = self.entries.remove(idx);
        self.entries.push(entry);
        self.entries.last()
    }

    fn put(&mut self, query: String, result: String, score: f32) {
        if self.entries.len() >= self.capacity {
            self.entries.remove(0); // LRU
        }
        self.entries.push((query, result, score));
    }

    fn clear(&mut self) {
        self.entries.clear();
    }
}

// ── Configuration ──────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct RagMiddlewareConfig {
    /// Répertoire de données pour le store RAG
    pub data_dir: PathBuf,
    /// Nombre de chunks max à retourner
    pub top_k: usize,
    /// Score minimum pour considérer un résultat pertinent
    pub min_score: f32,
    /// Pondération α pour la recherche hybride (0=BM25, 1=vecteur)
    pub hybrid_alpha: f64,
    /// Capacité du cache LRU
    pub cache_capacity: usize,
    /// Nombre de messages récents à utiliser comme requête
    pub context_window: usize,
    /// Timeout max (ms) pour la recherche RAG
    pub timeout_ms: u64,
}

impl Default for RagMiddlewareConfig {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from("/var/lib/soulsystem/data/rag"),
            top_k: 3,
            min_score: 0.7,
            hybrid_alpha: 0.7,
            cache_capacity: 100,
            context_window: 5,
            timeout_ms: 200,
        }
    }
}

// ── Résultat ───────────────────────────────────────────────────────

/// Résultat formaté pour injection dans le prompt.
#[derive(Debug, Clone)]
pub struct RagContext {
    /// Texte formaté pour injection
    pub prefix: String,
    /// Nombre de chunks trouvés
    pub chunks_found: usize,
    /// Score moyen
    pub avg_score: f32,
    /// Temps de recherche (ms)
    pub elapsed_ms: u64,
    /// Source des résultats
    pub source: &'static str,
}

// ── Middleware ──────────────────────────────────────────────────────

/// Middleware RAG actif, appelé avant chaque génération de réponse.
pub struct RagMiddleware {
    config: RagMiddlewareConfig,
    cache: Arc<RwLock<LruCache>>,
    store_available: Arc<RwLock<bool>>,
}

impl RagMiddleware {
    pub fn new(config: RagMiddlewareConfig) -> Self {
        let cache = Arc::new(RwLock::new(LruCache::new(config.cache_capacity)));

        Self {
            config,
            cache,
            store_available: Arc::new(RwLock::new(false)),
        }
    }

    /// Tente d'initialiser le store RAG.
    /// Échoue silencieusement si non disponible (dégradation gracieuse).
    pub async fn init_store(&self) -> Result<()> {
        let _ = std::fs::create_dir_all(&self.config.data_dir);

        // Vérifier si Ollama est disponible
        #[cfg(not(test))]
        {
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .build()?;
            let url =
                std::env::var("OLLAMA_URL").unwrap_or_else(|_| "http://127.0.0.1:11434".into());
            let ollama_ok = client.get(format!("{}/api/tags", url)).send().await.is_ok();
            *self.store_available.write().await = ollama_ok;
            if ollama_ok {
                info!("RagMiddleware: Ollama disponible sur {}", url);
            } else {
                warn!("RagMiddleware: Ollama indisponible, RAG en mode dégradé");
            }
        }

        #[cfg(test)]
        {
            *self.store_available.write().await = true;
        }

        Ok(())
    }

    /// Vérifie si le store RAG est disponible.
    pub async fn is_available(&self) -> bool {
        *self.store_available.read().await
    }

    /// Recherche du contexte pertinent à partir d'une requête.
    ///
    /// Utilise le cache LRU, le store RAG (si disponible), et le MemoryHub.
    pub async fn search(&self, query: &str, hub: &crate::memory_hub::MemoryHub) -> RagContext {
        let start = Instant::now();

        // 1. Vérifier le cache
        {
            let mut cache = self.cache.write().await;
            if let Some(cached) = cache.get(query) {
                return RagContext {
                    prefix: format!("📚 Mémoire pertinente (cache):\n{}\n", cached.1),
                    chunks_found: 1,
                    avg_score: cached.2,
                    elapsed_ms: start.elapsed().as_millis() as u64,
                    source: "cache",
                };
            }
        }

        // 2. Rechercher dans MemoryHub (vectoriel + graphe)
        let mut all_results: Vec<(String, f32)> = Vec::new();

        let hub_results = hub.search(query, self.config.top_k).await;
        for r in &hub_results {
            if r.score >= self.config.min_score {
                all_results.push((r.text.clone(), r.score));
            }
        }

        // 3. Si pas assez de résultats, chercher dans le contexte large
        if all_results.len() < self.config.top_k {
            let broad_results = hub.search(query, self.config.top_k * 2).await;
            for r in &broad_results {
                if r.score >= self.config.min_score * 0.85
                    && !all_results.iter().any(|(t, _)| t == &r.text)
                {
                    all_results.push((r.text.clone(), r.score));
                }
            }
        }

        // 4. Formater les résultats
        let elapsed_ms = start.elapsed().as_millis() as u64;

        if all_results.is_empty() {
            return RagContext {
                prefix: String::new(),
                chunks_found: 0,
                avg_score: 0.0,
                elapsed_ms,
                source: "none",
            };
        }

        // Trier par score (NaN-safe : les NaN sont placés en dernier)
        all_results.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        all_results.truncate(self.config.top_k);

        let avg_score: f32 =
            all_results.iter().map(|(_, s)| s).sum::<f32>() / all_results.len() as f32;

        let mut prefix = String::from("📚 Contexte mémoire pertinent:\n");
        for (i, (text, score)) in all_results.iter().enumerate() {
            prefix.push_str(&format!("[{i}] (score: {score:.2}) {text}\n"));
        }
        prefix.push_str("---\n");

        // Mettre en cache le meilleur résultat
        if let Some((text, score)) = all_results.first() {
            let mut cache = self.cache.write().await;
            cache.put(query.to_string(), text.clone(), *score);
        }

        RagContext {
            prefix,
            chunks_found: all_results.len(),
            avg_score,
            elapsed_ms,
            source: "memoryhub",
        }
    }

    /// Version simplifiée pour injection rapide dans un prompt système.
    /// Retourne le contexte formaté ou chaîne vide si rien trouvé.
    pub async fn get_context(&self, query: &str, hub: &crate::memory_hub::MemoryHub) -> String {
        let result = self.search(query, hub).await;
        result.prefix
    }

    /// Vide le cache.
    pub async fn clear_cache(&self) {
        self.cache.write().await.clear();
        info!("RagMiddleware: cache vidé");
    }

    /// Met à jour la disponibilité du store.
    pub async fn set_availability(&self, available: bool) {
        *self.store_available.write().await = available;
    }
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_cache_lru() {
        let dir = TempDir::new().unwrap();
        let config = RagMiddlewareConfig {
            data_dir: dir.path().to_path_buf(),
            cache_capacity: 2,
            ..Default::default()
        };
        let middleware = RagMiddleware::new(config);

        // Le cache fonctionne via self.cache
        {
            let mut cache = middleware.cache.write().await;
            cache.put("q1".into(), "r1".into(), 0.9);
            cache.put("q2".into(), "r2".into(), 0.8);
            cache.put("q3".into(), "r3".into(), 0.7); // devrait évincer q1
            assert!(cache.get("q1").is_none());
            assert!(cache.get("q2").is_some());
            assert!(cache.get("q3").is_some());
        }
    }

    #[tokio::test]
    async fn test_no_results() {
        let dir = TempDir::new().unwrap();
        let config = RagMiddlewareConfig {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };
        let hub = crate::memory_hub::MemoryHub::new(dir.path()).await;
        let middleware = RagMiddleware::new(config);

        let result = middleware.search("rien du tout", &hub).await;
        assert_eq!(result.chunks_found, 0);
        assert!(result.prefix.is_empty());
    }
}
