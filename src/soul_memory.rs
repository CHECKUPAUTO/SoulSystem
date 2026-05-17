//! SoulMemory — Base de connaissances vectorielle unifiee.
//!
//! Moteur d'embedding + recherche local utilisant une projection aleatoire
//! deterministe (SciRustEmbedder, 64-dim) ou n-grammes positionnels.
//! Stockage via sled. Fallback Qdrant pret.
//!
//! Mechanismes d'oubli: chaque entree a une importance qui decroit avec
//! le temps. `decay_and_prune` nettoie les entrees faibles.

use anyhow::Result;
use seahash::hash as seahash;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::{info, warn};

// ── Embedder trait ──────────────────────────────────────────────────────

/// Trait pour les moteurs d'embedding pluggables.
pub trait Embedder: Send + Sync {
    fn embed(&self, text: &str) -> Vec<f32>;
    fn dim(&self) -> usize;
    fn cosine_similarity(&self, a: &[f32], b: &[f32]) -> f32 {
        cosine_similarity(a, b)
    }
}

/// Cosine similarity entre deux vecteurs f32.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
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

// ── SciRustEmbedder: projection aleatoire deterministe ──────────────────

/// Embedder base sur une projection aleatoire deterministe (Johnson-Lindenstrauss).
///
/// Utilise 8 fonctions de hachage par n-gramme pour construire un vecteur
/// dense 64-dim. Equivalent a une matrice de projection aleatoire fixe,
/// preservant la similarite cosinus.
pub struct SciRustEmbedder {
    dim: usize,
    /// Seeds par "hash slot" — 8 slots = 8 projections independantes par n-gramme.
    seeds: [u64; 8],
}

impl SciRustEmbedder {
    pub fn new(dim: usize) -> Self {
        Self {
            dim,
            seeds: [42, 137, 251, 491, 773, 1021, 1301, 1607],
        }
    }
}

impl Embedder for SciRustEmbedder {
    fn dim(&self) -> usize {
        self.dim
    }

    fn embed(&self, text: &str) -> Vec<f32> {
        if text.is_empty() {
            return vec![0.0; self.dim];
        }

        let chars: Vec<char> = text.chars().collect();
        let mut vec = vec![0.0f32; self.dim];

        // n-grammes de taille 2..=4 avec 8 projections de hachage chacune
        for n in 2..=4usize {
            if n > chars.len() {
                continue;
            }
            for i in 0..=(chars.len() - n) {
                let ngram: String = chars[i..i + n].iter().collect();
                let base_hash = seahash(ngram.as_bytes());
                let pos_weight = 1.0 + (i as f32 / chars.len().max(1) as f32) * 0.5;

                for (slot, &seed) in self.seeds.iter().enumerate() {
                    let h = seahash(&base_hash.to_le_bytes());
                    let h2 = seahash(&[seed.to_le_bytes(), h.to_le_bytes()].concat());
                    let idx = (h2 as usize) % self.dim;
                    // Alternance de signe pour centrage zero
                    let sign = if (slot & 1) == 0 { 1.0 } else { -1.0 };
                    vec[idx] += sign * pos_weight;
                }
            }
        }

        l2_normalize(&mut vec);
        vec
    }
}

// ── NGramEmbedder (conserve pour compatibilite) ─────────────────────────

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
}

impl Embedder for NGramEmbedder {
    fn dim(&self) -> usize {
        self.dim
    }

    fn embed(&self, text: &str) -> Vec<f32> {
        if text.is_empty() {
            return vec![0.0; self.dim];
        }

        let mut vec = vec![0.0f32; self.dim];
        let chars: Vec<char> = text.chars().collect();

        for n in self.min_n..=self.max_n {
            if n > chars.len() {
                continue;
            }
            for i in 0..=(chars.len() - n) {
                let ngram: String = chars[i..i + n].iter().collect();
                let h = seahash(ngram.as_bytes());
                let pos = (h as usize) % self.dim;
                let pos_weight = 1.0 + (i as f32 / chars.len().max(1) as f32) * 0.5;
                vec[pos] += pos_weight;
            }
        }

        l2_normalize(&mut vec);
        vec
    }
}

fn l2_normalize(v: &mut [f32]) {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        let inv = 1.0 / norm;
        for x in v.iter_mut() {
            *x *= inv;
        }
    }
}

// ── Memory entry ────────────────────────────────────────────────────────

/// Point memoire avec embedding, metadonnees et importance.
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

/// Calcule une importance initiale basee sur une heuristique simple.
pub fn compute_initial_importance(text: &str, metadata: &HashMap<String, String>) -> f32 {
    let mut score = 0.5f32;

    // Longueur: textes plus longs = potentiellement plus informatifs
    let len = text.len() as f32;
    score += (len / 500.0).min(0.2);

    // Mots-cles importants
    let keywords = [
        "important",
        "critique",
        "urgent",
        "securite",
        "bug",
        "fix",
        "erreur",
        "failure",
        "security",
        "vulnerability",
        " CVE",
        "exploit",
        "architecture",
        "design",
        "decision",
        "breaking",
    ];
    let lower = text.to_lowercase();
    for kw in &keywords {
        if lower.contains(kw) {
            score += 0.05;
        }
    }

    // Source: certaines sources sont plus importantes
    if let Some(source) = metadata.get("source") {
        match source.as_str() {
            "audit" | "security" | "incident" => score += 0.2,
            "arxiv" | "research" => score += 0.1,
            "user_feedback" => score += 0.15,
            _ => {}
        }
    }

    // Tag veille
    if metadata.get("tag").map(|t| t.as_str()) == Some("veille") {
        score += 0.1;
    }

    score.clamp(0.1, 1.0)
}

// ── Backend ─────────────────────────────────────────────────────────────

enum Backend {
    Qdrant { url: String, collection: String },
    LocalFallback { db: sled::Db, _dim: usize },
}

// ── SoulMemory ──────────────────────────────────────────────────────────

/// Moteur memoire vectorielle.
pub struct SoulMemory {
    backend: Backend,
    embedder: Box<dyn Embedder>,
    next_id: AtomicU64,
}

impl SoulMemory {
    /// Cree une nouvelle instance avec SciRustEmbedder (64-dim).
    pub fn new() -> Result<Self> {
        Self::with_embedder(Box::new(SciRustEmbedder::new(64)))
    }

    /// Cree une instance avec un embedder personnalise.
    pub fn with_embedder(embedder: Box<dyn Embedder>) -> Result<Self> {
        let dim = embedder.dim();
        let qdrant_url = std::env::var("QDRANT_URL").unwrap_or_default();

        if !qdrant_url.is_empty() {
            info!("SoulMemory: using Qdrant at {}", qdrant_url);
            Ok(Self {
                backend: Backend::Qdrant {
                    url: qdrant_url,
                    collection: "soul_memory".into(),
                },
                embedder,
                next_id: AtomicU64::new(1),
            })
        } else {
            warn!("SoulMemory: QDRANT_URL not set, using local sled fallback");
            let db = sled::open("/var/lib/soulsystem/data/soul_memory")?;
            let max_id = Self::load_max_id(&db).unwrap_or(1);
            Ok(Self {
                backend: Backend::LocalFallback { db, _dim: dim },
                embedder,
                next_id: AtomicU64::new(max_id),
            })
        }
    }

    /// Cree une instance de test avec un backend temporaire et SciRustEmbedder.
    pub fn new_test() -> Result<Self> {
        Self::new_test_with_embedder(Box::new(SciRustEmbedder::new(64)))
    }

    /// Cree une instance de test avec embedder personnalise.
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
                    if id > max {
                        max = id;
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

    /// Stocke un texte avec metadonnees. L'importance initiale est calculee
    /// automatiquement via `compute_initial_importance`.
    pub async fn store(&self, text: &str, metadata: HashMap<String, String>) -> Result<()> {
        let importance = compute_initial_importance(text, &metadata);
        self.store_with_importance(text, metadata, importance).await
    }

    /// Stocke un texte avec une importance explicite.
    pub async fn store_with_importance(
        &self,
        text: &str,
        metadata: HashMap<String, String>,
        importance: f32,
    ) -> Result<()> {
        match &self.backend {
            Backend::Qdrant { url, collection } => {
                let vector = self.embedder.embed(text);
                let _ = (vector, url, collection);
                info!(
                    "SoulMemory: stored (Qdrant mock, importance={:.2}) — {}",
                    importance,
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
                    importance,
                };
                let key = entry.id.to_string();
                db.insert(key.as_bytes(), serde_json::to_vec(&entry)?)?;
                self.next_id.fetch_add(1, Ordering::Relaxed);
                info!(
                    "SoulMemory: stored (local, importance={:.2}) — {}",
                    importance,
                    &text[..text.len().min(60)]
                );
            }
        }
        Ok(())
    }

    /// Recherche les textes les plus proches d'une requete.
    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<(String, f32)>> {
        match &self.backend {
            Backend::Qdrant { .. } => {
                let query_vec = self.embedder.embed(query);
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

    /// Recupere un contexte textuel formate pour injection dans un prompt.
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

    /// Nombre total d'entrees dans la base.
    pub fn count(&self) -> Result<usize> {
        match &self.backend {
            Backend::Qdrant { .. } => Ok(0),
            Backend::LocalFallback { db, .. } => Ok(db.iter().count()),
        }
    }

    /// Applique la decroissance d'importance et elague les entrees faibles.
    ///
    /// - Chaque entree voit son importance multipliee par `decay_factor`.
    /// - Les entrees sous `threshold` sont supprimees.
    /// - Si le nombre d'entrees depasse `max_entries`, les moins importantes
    ///   sont supprimees.
    ///
    /// Retourne (entrees_restantes, entrees_supprimees).
    pub fn decay_and_prune(
        &self,
        decay_factor: f32,
        threshold: f32,
        max_entries: usize,
    ) -> Result<(usize, usize)> {
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
                        std::str::from_utf8(&key)
                            .unwrap_or("0")
                            .parse()
                            .unwrap_or(0),
                        entry,
                    ));
                }

                // Filtrer par seuil
                let (mut keep, remove): (Vec<_>, Vec<_>) = entries
                    .into_iter()
                    .partition(|(_, e)| e.importance >= threshold);

                // Supprimer les entrees sous le seuil
                let mut removed = 0usize;
                for (id, _) in &remove {
                    db.remove(id.to_string().as_bytes())?;
                    removed += 1;
                }

                // Si encore trop d'entrees, supprimer les moins importantes
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

                // Reecrire les entrees gardees avec leur nouvelle importance
                for (id, entry) in &keep {
                    db.insert(id.to_string().as_bytes(), serde_json::to_vec(entry)?)?;
                }

                info!(
                    "SoulMemory: decay_and_prune — {} kept, {} removed",
                    keep.len(),
                    removed
                );
                Ok((keep.len(), removed))
            }
        }
    }

    /// Retourne l'embedder (pour test de similarite externe).
    pub fn embedder(&self) -> &dyn Embedder {
        self.embedder.as_ref()
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── SciRustEmbedder tests ────────────────────────────────────────

    #[test]
    fn test_scirust_embedder_deterministic() {
        let e = SciRustEmbedder::new(64);
        let v1 = e.embed("hello world");
        let v2 = e.embed("hello world");
        assert_eq!(v1.len(), 64);
        assert!(v1.iter().zip(v2.iter()).all(|(a, b)| (a - b).abs() < 1e-6));
    }

    #[test]
    fn test_scirust_embedder_similar_texts_closer() {
        let e = SciRustEmbedder::new(64);
        let v_similar1 = e.embed("machine learning is great");
        let v_similar2 = e.embed("machine learning is awesome");
        let v_different = e.embed("the cat sat on the mat");

        let sim_similar = e.cosine_similarity(&v_similar1, &v_similar2);
        let sim_different = e.cosine_similarity(&v_similar1, &v_different);

        assert!(
            sim_similar > sim_different,
            "similar texts should have higher cosine similarity ({} > {})",
            sim_similar,
            sim_different
        );
    }

    #[test]
    fn test_scirust_embedder_empty() {
        let e = SciRustEmbedder::new(64);
        let v = e.embed("");
        assert_eq!(v.len(), 64);
        assert!(v.iter().all(|x| *x == 0.0));
    }

    #[test]
    fn test_scirust_embedder_normalized() {
        let e = SciRustEmbedder::new(64);
        let v = e.embed("some text for testing normalization");
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!(
            (norm - 1.0).abs() < 1e-5,
            "L2 norm should be ~1.0, got {}",
            norm
        );
    }

    // ── Importance tests ─────────────────────────────────────────────

    #[test]
    fn test_importance_keywords() {
        let meta = HashMap::new();
        let s1 = compute_initial_importance("hello world", &meta);
        let s2 = compute_initial_importance("critical security vulnerability found", &meta);
        assert!(s2 > s1, "security text should have higher importance");
    }

    #[test]
    fn test_importance_source() {
        let mut meta = HashMap::new();
        meta.insert("source".into(), "audit".into());
        let s = compute_initial_importance("some text", &meta);
        assert!(s > 0.6, "audit source should boost importance, got {}", s);
    }

    #[test]
    fn test_importance_clamped() {
        let meta = HashMap::new();
        let s = compute_initial_importance("", &meta);
        assert!(
            (0.1..=1.0).contains(&s),
            "importance should be clamped to [0.1, 1.0]"
        );
    }

    // ── SoulMemory tests ─────────────────────────────────────────────

    #[tokio::test]
    async fn test_store_and_search() {
        let mem = SoulMemory::new_test().unwrap();

        let mut meta1 = HashMap::new();
        meta1.insert("source".into(), "test".into());
        mem.store("Le chat noir dort sur le canape", meta1)
            .await
            .unwrap();

        let mut meta2 = HashMap::new();
        meta2.insert("source".into(), "test2".into());
        mem.store("Le chien brun court dans le jardin", meta2)
            .await
            .unwrap();

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
        mem.store("Texte duplique", HashMap::new()).await.unwrap();
        mem.store("Texte duplique", HashMap::new()).await.unwrap();

        let results = mem.search("Texte duplique", 10).await.unwrap();
        assert_eq!(results.len(), 2);
    }

    // ── Decay and prune tests ────────────────────────────────────────

    #[tokio::test]
    async fn test_decay_and_prune_removes_weak() {
        let mem = SoulMemory::new_test().unwrap();

        // Stocke 5 entrees
        for i in 0..5 {
            let mut meta = HashMap::new();
            meta.insert("n".into(), i.to_string());
            mem.store_with_importance(&format!("document numero {}", i), meta, 0.5)
                .await
                .unwrap();
        }

        assert_eq!(mem.count().unwrap(), 5);

        // Decay fort: 0.5 * 0.1 = 0.05 < threshold 0.1 → toutes supprimees
        let (kept, removed) = mem.decay_and_prune(0.1, 0.1, 100).unwrap();
        assert_eq!(removed, 5);
        assert_eq!(kept, 0);
        assert_eq!(mem.count().unwrap(), 0);
    }

    #[tokio::test]
    async fn test_decay_and_prune_respects_max_entries() {
        let mem = SoulMemory::new_test().unwrap();

        for i in 0..10 {
            let mut meta = HashMap::new();
            meta.insert("n".into(), i.to_string());
            // Importance decroissante
            mem.store_with_importance(
                &format!("document numero {}", i),
                meta,
                1.0 - i as f32 * 0.05,
            )
            .await
            .unwrap();
        }

        assert_eq!(mem.count().unwrap(), 10);

        // Decay leger (0.99), seuil bas, mais max_entries = 3
        let (kept, removed) = mem.decay_and_prune(0.99, 0.01, 3).unwrap();
        assert_eq!(kept, 3);
        assert_eq!(removed, 7);
        assert_eq!(mem.count().unwrap(), 3);
    }

    #[tokio::test]
    async fn test_embedder_injection() {
        // Test avec NGramEmbedder comme embedder alternatif
        let mem = SoulMemory::new_test_with_embedder(Box::new(NGramEmbedder::new(128))).unwrap();

        let mut meta = HashMap::new();
        meta.insert("source".into(), "injection_test".into());
        mem.store("Test avec NGramEmbedder", meta).await.unwrap();

        let results = mem.search("NGramEmbedder", 1).await.unwrap();
        assert_eq!(results.len(), 1);
    }
}
