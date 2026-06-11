//! soullink-memory-hierarchy — Episodic vs Semantic memory consolidation.
//!
//! Implements a three-layer memory hierarchy inspired by cognitive science:
//!
//! ```text
//!   Working Memory (bounded ring buffer, fast access)
//!       │
//!       ▼
//!   Episodic Memory (time-stamped events, fast decay)
//!       │  consolidation
//!       ▼
//!   Semantic Memory (facts/concepts, slow decay, graph-linked)
//! ```
//!
//! # Architecture
//!
//! - **WorkingMemory**: Bounded ring buffer of recent context items (default: 32).
//!   Serves as the active "scratchpad" for current task context.
//!
//! - **EpisodicStore**: Time-stamped event log with configurable decay.
//!   Important episodes are promoted to semantic memory via consolidation.
//!
//! - **SemanticStore**: Long-term fact storage with graph-based associations.
//!   Facts have types (Fact, Skill, Person, Project) and weighted edges.
//!
//! - **ConsolidationEngine**: Background task that periodically reviews
//!   recent episodic memories, clusters similar ones, and promotes
//!   important patterns to semantic storage.

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

// ── Memory Types ────────────────────────────────────────────────────────

/// A memory entry in any layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    /// Unique ID.
    pub id: String,
    /// Content text.
    pub text: String,
    /// When this memory was created.
    pub created_at: String,
    /// When this memory was last accessed.
    pub last_accessed: String,
    /// Number of times accessed.
    pub access_count: u64,
    /// Importance score [0.0, 1.0].
    pub importance: f32,
    /// Source layer.
    pub layer: MemoryLayer,
    /// Tags for categorization.
    pub tags: Vec<String>,
    /// Embedding vector (optional, for semantic search).
    #[serde(default)]
    pub embedding: Option<Vec<f32>>,
    /// Associated concept IDs (for graph linking).
    #[serde(default)]
    pub associations: Vec<String>,
    /// Metadata.
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Which memory layer a entry belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MemoryLayer {
    /// Active context (bounded ring buffer).
    Working,
    /// Time-stamped events (fast decay).
    Episodic,
    /// Long-term facts/concepts (slow decay).
    Semantic,
}

/// Concept type for semantic memory entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ConceptType {
    /// Generic fact.
    Fact,
    /// Learned skill or competence.
    Skill,
    /// Person or user.
    Person,
    /// Project entity.
    Project,
    /// Time-stamped event.
    Event,
    /// Neural-derived concept.
    Neural,
}

// ── Working Memory ──────────────────────────────────────────────────────

/// Bounded ring buffer for active context.
///
/// Evicts oldest entries when full. No persistence — resets on restart.
pub struct WorkingMemory {
    entries: Vec<MemoryEntry>,
    capacity: usize,
}

impl WorkingMemory {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: Vec::with_capacity(capacity),
            capacity,
        }
    }

    /// Add a memory to working memory.
    pub fn push(&mut self, entry: MemoryEntry) {
        if self.entries.len() >= self.capacity {
            self.entries.remove(0);
        }
        self.entries.push(entry);
    }

    /// Get all entries (most recent last).
    pub fn entries(&self) -> &[MemoryEntry] {
        &self.entries
    }

    /// Search working memory by text similarity (simple substring).
    pub fn search(&self, query: &str) -> Vec<&MemoryEntry> {
        let q = query.to_lowercase();
        self.entries
            .iter()
            .filter(|e| e.text.to_lowercase().contains(&q))
            .collect()
    }

    /// Clear all entries.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether working memory is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// ── Episodic Store ──────────────────────────────────────────────────────

/// Configuration for episodic memory decay.
#[derive(Debug, Clone)]
pub struct EpisodicConfig {
    /// Half-life in seconds (default: 86400 = 1 day).
    pub half_life_secs: u64,
    /// Minimum importance to keep (default: 0.1).
    pub min_importance: f32,
    /// Maximum entries before forced eviction (default: 10000).
    pub max_entries: usize,
}

impl Default for EpisodicConfig {
    fn default() -> Self {
        Self {
            half_life_secs: 86400,
            min_importance: 0.1,
            max_entries: 10000,
        }
    }
}

/// Time-stamped episodic memory store.
pub struct EpisodicStore {
    entries: DashMap<String, MemoryEntry>,
    config: EpisodicConfig,
}

impl EpisodicStore {
    pub fn new(config: EpisodicConfig) -> Self {
        Self {
            entries: DashMap::new(),
            config,
        }
    }

    /// Store an episodic memory.
    pub fn store(&self, mut entry: MemoryEntry) {
        entry.layer = MemoryLayer::Episodic;
        if entry.created_at.is_empty() {
            entry.created_at = Utc::now().to_rfc3339();
        }
        entry.last_accessed = entry.created_at.clone();
        self.entries.insert(entry.id.clone(), entry);
    }

    /// Search episodic memory by text.
    pub fn search(&self, query: &str, limit: usize) -> Vec<MemoryEntry> {
        let q = query.to_lowercase();
        let mut results: Vec<MemoryEntry> = self
            .entries
            .iter()
            .filter(|e| e.value().text.to_lowercase().contains(&q))
            .map(|e| e.value().clone())
            .collect();

        results.sort_by(|a, b| {
            b.importance
                .partial_cmp(&a.importance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(limit);
        results
    }

    /// Get recent episodic memories.
    pub fn recent(&self, limit: usize) -> Vec<MemoryEntry> {
        let mut entries: Vec<MemoryEntry> =
            self.entries.iter().map(|e| e.value().clone()).collect();
        entries.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        entries.truncate(limit);
        entries
    }

    /// Apply time-based decay to all entries.
    ///
    /// Returns the number of entries removed.
    pub fn decay(&self) -> usize {
        let now = Utc::now();
        let half_life = std::time::Duration::from_secs(self.config.half_life_secs);
        let mut removed = 0;

        let keys: Vec<String> = self.entries.iter().map(|e| e.key().clone()).collect();
        for key in keys {
            if let Some(mut entry) = self.entries.get_mut(&key) {
                if let Ok(created) = DateTime::parse_from_rfc3339(&entry.created_at) {
                    let age = now.signed_duration_since(created.with_timezone(&Utc));
                    let age_secs = age.num_seconds().max(0) as f64;
                    let half_life_secs = half_life.as_secs() as f64;

                    // Exponential decay
                    let decay = 0.5_f64.powf(age_secs / half_life_secs);
                    entry.importance *= decay as f32;

                    if entry.importance < self.config.min_importance {
                        drop(entry);
                        self.entries.remove(&key);
                        removed += 1;
                    }
                }
            }
        }

        // Force eviction if over max
        if self.entries.len() > self.config.max_entries {
            let excess = self.entries.len() - self.config.max_entries;
            let mut to_evict: Vec<(String, f32)> = self
                .entries
                .iter()
                .map(|e| (e.key().clone(), e.value().importance))
                .collect();
            to_evict.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
            for (key, _) in to_evict.into_iter().take(excess) {
                self.entries.remove(&key);
                removed += 1;
            }
        }

        removed
    }

    /// Promote an entry to semantic memory (removes from episodic).
    pub fn promote(&self, id: &str) -> Option<MemoryEntry> {
        self.entries.remove(id).map(|(_, mut e)| {
            e.layer = MemoryLayer::Semantic;
            e
        })
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True si le store est vide.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// ── Semantic Store ──────────────────────────────────────────────────────

/// Configuration for semantic memory.
#[derive(Debug, Clone)]
pub struct SemanticConfig {
    /// Half-life in seconds (default: 2592000 = 30 days).
    pub half_life_secs: u64,
    /// Minimum importance to keep (default: 0.05).
    pub min_importance: f32,
}

impl Default for SemanticConfig {
    fn default() -> Self {
        Self {
            half_life_secs: 2592000, // 30 days
            min_importance: 0.05,
        }
    }
}

/// Long-term semantic memory with concept types and associations.
pub struct SemanticStore {
    entries: DashMap<String, MemoryEntry>,
    /// Concept type index.
    by_type: DashMap<ConceptType, Vec<String>>,
    config: SemanticConfig,
}

impl SemanticStore {
    pub fn new(config: SemanticConfig) -> Self {
        Self {
            entries: DashMap::new(),
            by_type: DashMap::new(),
            config,
        }
    }

    /// Store a semantic memory.
    pub fn store(&self, mut entry: MemoryEntry, concept_type: ConceptType) {
        entry.layer = MemoryLayer::Semantic;
        if entry.created_at.is_empty() {
            entry.created_at = Utc::now().to_rfc3339();
        }
        entry.last_accessed = entry.created_at.clone();
        entry
            .metadata
            .insert("concept_type".into(), serde_json::json!(concept_type));

        let id = entry.id.clone();
        self.entries.insert(id.clone(), entry);

        self.by_type.entry(concept_type).or_default().push(id);
    }

    /// Search semantic memory by text.
    pub fn search(&self, query: &str, limit: usize) -> Vec<MemoryEntry> {
        let q = query.to_lowercase();
        let mut results: Vec<MemoryEntry> = self
            .entries
            .iter()
            .filter(|e| e.value().text.to_lowercase().contains(&q))
            .map(|e| e.value().clone())
            .collect();

        results.sort_by(|a, b| {
            b.importance
                .partial_cmp(&a.importance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(limit);
        results
    }

    /// Get entries by concept type.
    pub fn by_type(&self, concept_type: ConceptType) -> Vec<MemoryEntry> {
        self.by_type
            .get(&concept_type)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.entries.get(id).map(|e| e.value().clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Access a memory (bumps access_count and last_accessed).
    pub fn access(&self, id: &str) {
        if let Some(mut entry) = self.entries.get_mut(id) {
            entry.access_count += 1;
            entry.last_accessed = Utc::now().to_rfc3339();
            // Boost importance on access
            entry.importance = (entry.importance + 0.05).min(1.0);
        }
    }

    /// Apply time-based decay.
    pub fn decay(&self) -> usize {
        let now = Utc::now();
        let half_life = std::time::Duration::from_secs(self.config.half_life_secs);
        let mut removed = 0;

        let keys: Vec<String> = self.entries.iter().map(|e| e.key().clone()).collect();
        for key in keys {
            if let Some(mut entry) = self.entries.get_mut(&key) {
                if let Ok(created) = DateTime::parse_from_rfc3339(&entry.created_at) {
                    let age = now.signed_duration_since(created.with_timezone(&Utc));
                    let age_secs = age.num_seconds().max(0) as f64;
                    let half_life_secs = half_life.as_secs() as f64;

                    let decay = 0.5_f64.powf(age_secs / half_life_secs);
                    entry.importance *= decay as f32;

                    // Access count bonus: frequently accessed memories decay slower
                    let access_bonus = (entry.access_count as f32 * 0.01).min(0.3);
                    entry.importance += access_bonus;

                    if entry.importance < self.config.min_importance {
                        drop(entry);
                        self.entries.remove(&key);
                        removed += 1;
                    }
                }
            }
        }

        removed
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True si le store est vide.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// ── Consolidation Engine ────────────────────────────────────────────────

/// Configuration for memory consolidation.
#[derive(Debug, Clone)]
pub struct ConsolidationConfig {
    /// How often to run consolidation (in seconds, default: 3600 = 1 hour).
    pub interval_secs: u64,
    /// Minimum importance for episodic entry to be promoted.
    pub min_promotion_importance: f32,
    /// Minimum number of similar episodic entries to cluster before promotion.
    pub min_cluster_size: usize,
    /// Text similarity threshold for clustering (substring match).
    pub similarity_threshold: f32,
}

impl Default for ConsolidationConfig {
    fn default() -> Self {
        Self {
            interval_secs: 3600,
            min_promotion_importance: 0.5,
            min_cluster_size: 3,
            similarity_threshold: 0.7,
        }
    }
}

/// Result of a consolidation cycle.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConsolidationResult {
    /// Number of episodic entries reviewed.
    pub reviewed: usize,
    /// Number of entries promoted to semantic.
    pub promoted: usize,
    /// Number of entries decayed away.
    pub decayed: usize,
    /// Number of clusters formed.
    pub clusters: usize,
}

/// Background consolidation engine.
pub struct ConsolidationEngine {
    config: ConsolidationConfig,
    episodic: Arc<EpisodicStore>,
    semantic: Arc<SemanticStore>,
}

impl ConsolidationEngine {
    pub fn new(
        config: ConsolidationConfig,
        episodic: Arc<EpisodicStore>,
        semantic: Arc<SemanticStore>,
    ) -> Self {
        Self {
            config,
            episodic,
            semantic,
        }
    }

    /// Run a single consolidation cycle.
    pub async fn consolidate(&self) -> ConsolidationResult {
        let mut result = ConsolidationResult {
            // 1. Decay episodic memories
            decayed: self.episodic.decay(),
            ..Default::default()
        };

        // 2. Get recent episodic entries above threshold
        let candidates: Vec<MemoryEntry> = self
            .episodic
            .entries
            .iter()
            .filter(|e| e.value().importance >= self.config.min_promotion_importance)
            .map(|e| e.value().clone())
            .collect();

        result.reviewed = candidates.len();

        // 3. Cluster similar entries by text overlap
        let clusters = self.cluster_entries(&candidates);
        result.clusters = clusters.len();

        // 4. Promote clusters that meet minimum size
        for cluster in &clusters {
            if cluster.len() >= self.config.min_cluster_size {
                // Merge cluster into a single semantic entry
                let merged = self.merge_cluster(cluster);
                self.semantic.store(merged, ConceptType::Fact);
                result.promoted += cluster.len();

                // Remove promoted entries from episodic
                for entry in cluster {
                    self.episodic.entries.remove(&entry.id);
                }
            }
        }

        // 5. Decay semantic memories too
        self.semantic.decay();

        info!(
            "Consolidation: reviewed={}, promoted={}, decayed={}, clusters={}",
            result.reviewed, result.promoted, result.decayed, result.clusters
        );

        result
    }

    /// Cluster entries by text similarity (simple substring overlap).
    fn cluster_entries(&self, entries: &[MemoryEntry]) -> Vec<Vec<MemoryEntry>> {
        let mut used = vec![false; entries.len()];
        let mut clusters = Vec::new();

        for i in 0..entries.len() {
            if used[i] {
                continue;
            }

            let mut cluster = vec![entries[i].clone()];
            used[i] = true;

            for j in (i + 1)..entries.len() {
                if used[j] {
                    continue;
                }

                if self.text_similarity(&entries[i].text, &entries[j].text)
                    >= self.config.similarity_threshold
                {
                    cluster.push(entries[j].clone());
                    used[j] = true;
                }
            }

            clusters.push(cluster);
        }

        clusters
    }

    /// Simple text similarity: ratio of shared words to total words.
    fn text_similarity(&self, a: &str, b: &str) -> f32 {
        let words_a: Vec<&str> = a.split_whitespace().collect();
        let words_b: Vec<&str> = b.split_whitespace().collect();

        if words_a.is_empty() || words_b.is_empty() {
            return 0.0;
        }

        let set_a: std::collections::HashSet<&str> = words_a.iter().copied().collect();
        let set_b: std::collections::HashSet<&str> = words_b.iter().copied().collect();

        let intersection = set_a.intersection(&set_b).count();
        let union = set_a.len().max(set_b.len());

        intersection as f32 / union as f32
    }

    /// Merge a cluster of similar entries into one semantic entry.
    fn merge_cluster(&self, cluster: &[MemoryEntry]) -> MemoryEntry {
        // Use the highest-importance entry as base
        let base = cluster
            .iter()
            .max_by(|a, b| {
                a.importance
                    .partial_cmp(&b.importance)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap();

        // Combine text: use the longest text as the representative
        let representative = cluster.iter().max_by_key(|e| e.text.len()).unwrap();

        // Combine tags
        let mut all_tags: Vec<String> = Vec::new();
        for entry in cluster {
            for tag in &entry.tags {
                if !all_tags.contains(tag) {
                    all_tags.push(tag.clone());
                }
            }
        }

        // Boost importance based on cluster size
        let cluster_boost = (cluster.len() as f32 * 0.05).min(0.3);

        MemoryEntry {
            id: format!("sem_{}", base.id),
            text: representative.text.clone(),
            created_at: base.created_at.clone(),
            last_accessed: Utc::now().to_rfc3339(),
            access_count: cluster.iter().map(|e| e.access_count).sum(),
            importance: (base.importance + cluster_boost).min(1.0),
            layer: MemoryLayer::Semantic,
            tags: all_tags,
            embedding: None,
            associations: cluster.iter().map(|e| e.id.clone()).collect(),
            metadata: HashMap::from([
                ("source".into(), serde_json::json!("consolidation")),
                ("cluster_size".into(), serde_json::json!(cluster.len())),
            ]),
        }
    }
}

// ── Hierarchical Memory Manager ─────────────────────────────────────────

/// Top-level manager coordinating all memory layers.
pub struct HierarchicalMemory {
    pub working: Arc<RwLock<WorkingMemory>>,
    pub episodic: Arc<EpisodicStore>,
    pub semantic: Arc<SemanticStore>,
    pub consolidation: Arc<ConsolidationEngine>,
}

impl HierarchicalMemory {
    pub fn new(
        working_capacity: usize,
        episodic_config: EpisodicConfig,
        semantic_config: SemanticConfig,
        consolidation_config: ConsolidationConfig,
    ) -> Self {
        let episodic = Arc::new(EpisodicStore::new(episodic_config));
        let semantic = Arc::new(SemanticStore::new(semantic_config));
        let consolidation = Arc::new(ConsolidationEngine::new(
            consolidation_config,
            Arc::clone(&episodic),
            Arc::clone(&semantic),
        ));

        Self {
            working: Arc::new(RwLock::new(WorkingMemory::new(working_capacity))),
            episodic,
            semantic,
            consolidation,
        }
    }

    /// Store a memory (auto-routes to appropriate layer).
    pub async fn store(&self, entry: MemoryEntry, layer: MemoryLayer) {
        match layer {
            MemoryLayer::Working => {
                self.working.write().await.push(entry);
            }
            MemoryLayer::Episodic => {
                self.episodic.store(entry);
            }
            MemoryLayer::Semantic => {
                self.semantic.store(entry, ConceptType::Fact);
            }
        }
    }

    /// Search across all layers.
    pub async fn search(&self, query: &str, limit: usize) -> Vec<MemoryEntry> {
        let mut results = Vec::new();

        // Working memory (substring match)
        {
            let working = self.working.read().await;
            for entry in working.search(query) {
                results.push(entry.clone());
            }
        }

        // Episodic
        results.extend(self.episodic.search(query, limit));

        // Semantic
        results.extend(self.semantic.search(query, limit));

        // Dedup by text
        results.sort_by(|a, b| {
            b.importance
                .partial_cmp(&a.importance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.dedup_by(|a, b| a.text == b.text);
        results.truncate(limit);

        results
    }

    /// Run consolidation cycle.
    pub async fn consolidate(&self) -> ConsolidationResult {
        self.consolidation.consolidate().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_working_memory_push_evict() {
        let mut wm = WorkingMemory::new(3);
        for i in 0..5 {
            wm.push(MemoryEntry {
                id: format!("w{}", i),
                text: format!("working memory item {}", i),
                created_at: String::new(),
                last_accessed: String::new(),
                access_count: 0,
                importance: 0.5,
                layer: MemoryLayer::Working,
                tags: vec![],
                embedding: None,
                associations: vec![],
                metadata: HashMap::new(),
            });
        }
        assert_eq!(wm.len(), 3);
        assert_eq!(wm.entries()[0].text, "working memory item 2");
    }

    #[test]
    fn test_episodic_store_decay() {
        let store = EpisodicStore::new(EpisodicConfig {
            half_life_secs: 1, // 1 second for testing
            min_importance: 0.1,
            max_entries: 100,
        });

        store.store(MemoryEntry {
            id: "old".into(),
            text: "old memory".into(),
            created_at: "2020-01-01T00:00:00Z".into(),
            last_accessed: String::new(),
            access_count: 0,
            importance: 0.5,
            layer: MemoryLayer::Episodic,
            tags: vec![],
            embedding: None,
            associations: vec![],
            metadata: HashMap::new(),
        });

        let removed = store.decay();
        assert!(removed >= 1);
    }

    #[test]
    fn test_semantic_store_by_type() {
        let store = SemanticStore::new(SemanticConfig::default());

        store.store(
            MemoryEntry {
                id: "s1".into(),
                text: "Rust is a systems language".into(),
                created_at: String::new(),
                last_accessed: String::new(),
                access_count: 0,
                importance: 0.8,
                layer: MemoryLayer::Semantic,
                tags: vec!["rust".into()],
                embedding: None,
                associations: vec![],
                metadata: HashMap::new(),
            },
            ConceptType::Fact,
        );

        store.store(
            MemoryEntry {
                id: "s2".into(),
                text: "Alice is a developer".into(),
                created_at: String::new(),
                last_accessed: String::new(),
                access_count: 0,
                importance: 0.7,
                layer: MemoryLayer::Semantic,
                tags: vec!["person".into()],
                embedding: None,
                associations: vec![],
                metadata: HashMap::new(),
            },
            ConceptType::Person,
        );

        let facts = store.by_type(ConceptType::Fact);
        assert_eq!(facts.len(), 1);
        assert!(facts[0].text.contains("Rust"));

        let people = store.by_type(ConceptType::Person);
        assert_eq!(people.len(), 1);
        assert!(people[0].text.contains("Alice"));
    }

    #[tokio::test]
    async fn test_consolidation_promotes() {
        let episodic = Arc::new(EpisodicStore::new(EpisodicConfig {
            half_life_secs: 86400,
            min_importance: 0.1,
            max_entries: 100,
        }));

        let semantic = Arc::new(SemanticStore::new(SemanticConfig::default()));

        // Store 3 similar high-importance entries
        for i in 0..3 {
            episodic.store(MemoryEntry {
                id: format!("e{}", i),
                text: "Rust ownership prevents data races at compile time".into(),
                created_at: Utc::now().to_rfc3339(),
                last_accessed: String::new(),
                access_count: 0,
                importance: 0.8,
                layer: MemoryLayer::Episodic,
                tags: vec!["rust".into()],
                embedding: None,
                associations: vec![],
                metadata: HashMap::new(),
            });
        }

        let engine = ConsolidationEngine::new(
            ConsolidationConfig {
                interval_secs: 3600,
                min_promotion_importance: 0.5,
                min_cluster_size: 2,
                similarity_threshold: 0.5,
            },
            Arc::clone(&episodic),
            Arc::clone(&semantic),
        );

        let result = engine.consolidate().await;
        assert!(result.promoted >= 3);
        assert!(episodic.is_empty());
        assert!(!semantic.is_empty());
    }
}
