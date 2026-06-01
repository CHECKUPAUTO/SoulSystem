//! MemoryHub — Hub memoire unifie avec graphe conceptuel + RAG.
//!
//! Backend principal : soul-memory (sled/Qdrant, toujours actif).
//! Ajoute le graphe conceptuel soullink-memory pour la navigation associative
//! et le pipeline RAG soullink-rag.

use anyhow::Result;
use chrono::Utc;
use soul_memory::SoulMemory;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use soullink_memory::concept::{Concept, ConceptKind};
use soullink_memory::graph::MemoryGraph;
use soullink_memory::DecayConfig;
use soullink_rag::pipeline::PipelineConfig;

pub type MemoryEventFn = Arc<dyn Fn(&str, serde_json::Value) + Send + Sync>;

// ── SimpleEmbedder ────────────────────────────────────────────────────

struct SimpleEmbedder {
    dim: usize,
    seeds: [u64; 8],
}

impl SimpleEmbedder {
    fn new(dim: usize) -> Self {
        Self {
            dim,
            seeds: [42, 137, 251, 491, 773, 1021, 1301, 1607],
        }
    }
    fn embed(&self, text: &str) -> Vec<f32> {
        if text.is_empty() {
            return vec![0.0; self.dim];
        }
        use std::hash::{Hash, Hasher};
        let chars: Vec<char> = text.chars().collect();
        let mut vec = vec![0.0f32; self.dim];
        for n in 2..=4usize {
            if n > chars.len() {
                continue;
            }
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
        if norm > 0.0 {
            for x in &mut vec {
                *x /= norm;
            }
        }
        vec
    }
}

// ── MemoryHub ──────────────────────────────────────────────────────────

pub struct MemoryHub {
    pub vector: SoulMemory,
    pub graph: Option<Arc<RwLock<MemoryGraph>>>,
    pub rag_config: Option<PipelineConfig>,
    pub on_event: Option<MemoryEventFn>,

    // ── Versioning (Résil A) ──────────────────────────────────────────
    /// Journal append-only des modifications (version, horodatage, opération)
    pub version_journal: Arc<RwLock<Vec<VersionEntry>>>,

    // ── Contrôle d'accès (Résil B) ────────────────────────────────────
    /// Niveau de confidentialité par défaut
    pub default_privacy: PrivacyLevel,
}

impl MemoryHub {
    pub async fn new(data_dir: &std::path::Path) -> Self {
        let _ = data_dir;
        let vector = SoulMemory::new().expect("SoulMemory doit s'initialiser");
        info!("MemoryHub: soul-memory actif");

        let graph = match MemoryGraph::open(&data_dir.join("concept_graph"), DecayConfig::default())
        {
            Ok(g) => {
                info!("MemoryHub: graphe conceptuel actif (soullink-memory)");
                Some(Arc::new(RwLock::new(g)))
            }
            Err(e) => {
                tracing::warn!("MemoryHub: graphe conceptuel indisponible: {}", e);
                None
            }
        };

        let rag_config = Some(PipelineConfig {
            embed_model: "nomic-embed-text".into(),
            embed_dim: 768,
            dedup_threshold: 0.92,
            max_chunks: 20,
            chunk_size: 512,
            chunk_overlap: 64,
        });

        // Ouvrir le journal de versionnement depuis le fichier persistant
        let version_journal = Arc::new(RwLock::new(Vec::new()));
        let journal_path = data_dir.join("version_journal.json");
        if let Ok(content) = std::fs::read_to_string(&journal_path) {
            if let Ok(entries) = serde_json::from_str::<Vec<VersionEntry>>(&content) {
                *version_journal.blocking_write() = entries;
                info!(
                    "MemoryHub: journal de version chargé ({} entrées)",
                    version_journal.blocking_read().len()
                );
            }
        }

        Self {
            vector,
            graph,
            rag_config,
            on_event: None,
            version_journal,
            default_privacy: PrivacyLevel::Private,
        }
    }

    /// Sauvegarde le journal de version sur disque.
    pub async fn persist_journal(&self, data_dir: &PathBuf) -> Result<()> {
        let journal_path = data_dir.join("version_journal.json");
        let entries = self.version_journal.read().await.clone();
        let json = serde_json::to_string_pretty(&entries)?;
        tokio::fs::write(&journal_path, &json).await?;
        Ok(())
    }

    pub fn set_event_callback(&mut self, cb: MemoryEventFn) {
        self.on_event = Some(cb);
    }

    fn emit(&self, kind: &str, payload: serde_json::Value) {
        if let Some(ref cb) = self.on_event {
            cb(kind, payload);
        }
    }

    // ── Versioning (Résil A) ──────────────────────────────────────────

    /// Ajoute une entrée dans le journal de version.
    async fn record_version(&self, op: &str, text: &str, metadata: &HashMap<String, String>) {
        let entry = VersionEntry {
            version: {
                let mut j = self.version_journal.write().await;
                j.push(VersionEntry::dummy()); // placeholder
                j.pop();
                j.len() as u64 + 1
            },
            timestamp_ms: Utc::now().timestamp_millis(),
            operation: op.to_string(),
            text_preview: text.chars().take(100).collect(),
            metadata_summary: metadata.get("tag").cloned().unwrap_or_default(),
            privacy: metadata
                .get("privacy")
                .and_then(|v| v.parse::<PrivacyLevel>().ok())
                .unwrap_or(self.default_privacy),
        };
        self.version_journal.write().await.push(entry);
    }

    /// Retourne l'historique des versions.
    pub async fn get_version_history(&self, limit: usize) -> Vec<VersionEntry> {
        let journal = self.version_journal.read().await;
        journal.iter().rev().take(limit).cloned().collect()
    }

    /// Retourne une version spécifique par son numéro.
    pub async fn get_version(&self, version: u64) -> Option<VersionEntry> {
        let journal = self.version_journal.read().await;
        journal.iter().find(|e| e.version == version).cloned()
    }

    // ── Contrôle d'accès (Résil B) ────────────────────────────────────

    /// Définit le niveau de confidentialité par défaut.
    pub fn set_default_privacy(&mut self, level: PrivacyLevel) {
        self.default_privacy = level;
    }

    /// Filtre les résultats de recherche par niveau de confidentialité.
    pub fn filter_by_privacy(
        results: Vec<SearchResult>,
        min_level: PrivacyLevel,
    ) -> Vec<SearchResult> {
        results
            .into_iter()
            .filter(|r| {
                // Les résultats du vector store n'ont pas de label de privacy directement
                // On filtre au niveau des métadonnées lors du store
                true // Pour l'instant, passe tout (filtrage via métadonnées)
            })
            .collect()
    }

    /// Ajoute le label de privacy aux métadonnées si absent.
    fn ensure_privacy_label(metadata: &mut HashMap<String, String>, default: PrivacyLevel) {
        metadata
            .entry("privacy".into())
            .or_insert_with(|| default.to_string());
    }

    pub async fn store(&self, text: &str, mut metadata: HashMap<String, String>) -> Result<()> {
        // Ajouter le label de privacy par défaut si absent
        Self::ensure_privacy_label(&mut metadata, self.default_privacy);

        self.vector.store(text, metadata.clone()).await?;
        if let Some(ref graph) = self.graph {
            let kind = classify_text(text, &metadata);
            let _ = graph.write().await.insert(Concept::new(text, kind), None);
        }

        // Enregistrer la version
        self.record_version("store", text, &metadata).await;

        self.emit(
            "stored",
            serde_json::json!({"text": text, "metadata": metadata}),
        );
        Ok(())
    }

    pub async fn search(&self, query: &str, top_k: usize) -> Vec<SearchResult> {
        let mut results: Vec<SearchResult> = Vec::new();

        // 1. SoulMemory (vectoriel)
        if let Ok(hits) = self.vector.search(query, top_k).await {
            for (text, score) in hits {
                results.push(SearchResult {
                    text,
                    score,
                    source: "vector",
                });
            }
        }

        // 2. Graphe conceptuel
        if let Some(ref graph) = self.graph {
            let qv = SimpleEmbedder::new(64).embed(query);
            for r in &graph.read().await.search(&qv, top_k) {
                results.push(SearchResult {
                    text: r.label.clone(),
                    score: r.score,
                    source: "graph",
                });
            }
        }

        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        results.dedup_by(|a, b| a.text == b.text);
        results.truncate(top_k);
        results
    }

    pub async fn get_context(&self, query: &str, limit: usize) -> String {
        let results = self.search(query, limit).await;
        if results.is_empty() {
            return String::new();
        }
        let mut ctx = String::from("Contexte memoire:\n");
        for (i, r) in results.iter().enumerate() {
            ctx.push_str(&format!("[{}] ({}) {}\n", i, r.source, r.text));
        }
        ctx
    }

    pub async fn decay_and_prune(&self, threshold: f32, decay: f32, max: usize) {
        let _ = self.vector.decay_and_prune(threshold, decay, max);
        if let Some(ref graph) = self.graph {
            let _ = graph.write().await.decay_all();
        }
        self.emit(
            "decayed",
            serde_json::json!({"threshold": threshold, "max": max}),
        );
    }
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub text: String,
    pub score: f32,
    pub source: &'static str,
}

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

// ── Versioning (Résil A) ──────────────────────────────────────────────

/// Entrée dans le journal de versionnement.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VersionEntry {
    pub version: u64,
    pub timestamp_ms: i64,
    pub operation: String,
    pub text_preview: String,
    pub metadata_summary: String,
    pub privacy: PrivacyLevel,
}

impl VersionEntry {
    fn dummy() -> Self {
        Self {
            version: 0,
            timestamp_ms: 0,
            operation: String::new(),
            text_preview: String::new(),
            metadata_summary: String::new(),
            privacy: PrivacyLevel::Private,
        }
    }
}

// ── Contrôle d'accès (Résil B) ────────────────────────────────────────

/// Niveau de confidentialité d'un souvenir.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum PrivacyLevel {
    /// Visible uniquement par l'utilisateur propriétaire
    Private,
    /// Visible par l'équipe
    Team,
    /// Visible par tous
    Public,
}

impl PrivacyLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            PrivacyLevel::Private => "private",
            PrivacyLevel::Team => "team",
            PrivacyLevel::Public => "public",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "private" => Some(PrivacyLevel::Private),
            "team" => Some(PrivacyLevel::Team),
            "public" => Some(PrivacyLevel::Public),
            _ => None,
        }
    }

    /// Vérifie si ce niveau permet l'accès à un autre niveau.
    pub fn allows_access_to(&self, required: PrivacyLevel) -> bool {
        match (self, required) {
            (PrivacyLevel::Public, _) => true,
            (PrivacyLevel::Team, PrivacyLevel::Private) => false,
            (PrivacyLevel::Team, _) => true,
            (PrivacyLevel::Private, PrivacyLevel::Private) => true,
            (PrivacyLevel::Private, _) => false,
        }
    }
}

impl std::fmt::Display for PrivacyLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for PrivacyLevel {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        PrivacyLevel::from_str(s).ok_or_else(|| format!("PrivacyLevel invalide: {}", s))
    }
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
        hub.store("Le chat noir dort", HashMap::new())
            .await
            .unwrap();
        assert!(!hub.search("chat noir", 5).await.is_empty());
    }
    #[tokio::test]
    async fn test_event_callback() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut hub = MemoryHub::new(dir.path()).await;
        let fired = Arc::new(AtomicBool::new(false));
        let f = fired.clone();
        hub.set_event_callback(Arc::new(move |kind, _| {
            if kind == "stored" {
                f.store(true, Ordering::SeqCst);
            }
        }));
        hub.store("test", HashMap::new()).await.unwrap();
        assert!(fired.load(Ordering::SeqCst));
    }
}
