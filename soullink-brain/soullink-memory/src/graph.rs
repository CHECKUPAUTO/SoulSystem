//! MemoryGraph — core data structure: concept graph + vector store + sled persistence.

use anyhow::{Context, Result};
use chrono::Utc;
use dashmap::DashMap;
use parking_lot::RwLock;
use petgraph::stable_graph::{NodeIndex, StableGraph};
use petgraph::visit::EdgeRef;
use petgraph::Undirected;
use sled::Db;
use soullink_vector::alignment::{align_vector, DimensionRegistry};
use soullink_vector::hnsw_index::{HnswParams, SearchResult, VectorStore};
use std::path::Path;

use crate::concept::Concept;
use crate::decay::DecayConfig;
use crate::persistence;

/// Serialized edge record for sled storage.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EdgeRecord {
    pub a: usize,
    pub b: usize,
    pub weight: f32,
}

/// Edge metadata kept in-memory alongside the petgraph edge.
#[derive(Debug, Clone)]
#[allow(dead_code)] // reserved for future edge aging logic
struct EdgeMeta {
    created_at: chrono::DateTime<Utc>,
}

/// The memory graph — concepts as nodes, weighted edges as associations.
pub struct MemoryGraph {
    /// The concept graph (StableGraph for safe deletion).
    graph: RwLock<StableGraph<Concept, f32, Undirected>>,
    /// Label → NodeIndex for O(1) lookup.
    index: DashMap<String, NodeIndex>,
    /// Vector store for semantic search.
    vectors: VectorStore,
    /// Dimension registry (from soullink-vector).
    dimensions: DimensionRegistry,
    /// Persistence via sled.
    db: Db,
    /// Decay config.
    decay: DecayConfig,
    /// Edge creation timestamps for decay calculation.
    edge_timestamps: DashMap<(usize, usize), chrono::DateTime<Utc>>,
}

impl MemoryGraph {
    /// Open or create a MemoryGraph backed by sled at `db_path`.
    pub fn open(db_path: &Path, decay: DecayConfig) -> Result<Self> {
        Self::open_with_cache(db_path, decay, 1024) // 1GB default cache
    }

    /// Open with custom sled cache size (MB). 10GB recommended for 125GB RAM systems.
    pub fn open_with_cache(db_path: &Path, decay: DecayConfig, cache_mb: usize) -> Result<Self> {
        let db = sled::Config::default()
            .path(db_path)
            .cache_capacity(cache_mb as u64 * 1024 * 1024)
            .open()
            .context("opening sled database")?;
        let vectors = VectorStore::new(HnswParams::default());
        let graph = StableGraph::<Concept, f32, petgraph::Undirected>::default();
        let mut mg = Self {
            graph: RwLock::new(graph),
            index: DashMap::new(),
            vectors,
            dimensions: DimensionRegistry::new(),
            db,
            decay,
            edge_timestamps: DashMap::new(),
        };
        mg.load_from_db()?;
        Ok(mg)
    }

    /// Open or create a MemoryGraph with an existing sled Db.
    /// Same as `open()` but uses a pre-configured database instead of opening one.
    pub fn open_with_db(db: Db, decay: DecayConfig) -> Result<Self> {
        let vectors = VectorStore::new(HnswParams::default());
        let graph = StableGraph::<Concept, f32, petgraph::Undirected>::default();
        let mut mg = Self {
            graph: RwLock::new(graph),
            index: DashMap::new(),
            vectors,
            dimensions: DimensionRegistry::new(),
            db,
            decay,
            edge_timestamps: DashMap::new(),
        };
        mg.load_from_db()?;
        Ok(mg)
    }

    /// Insert a concept, optionally with embedding vector.
    /// Validates dimension, aligns SIMD, stores in vector store + graph + sled.
    pub fn insert(&self, concept: Concept, vector: Option<Vec<f32>>) -> Result<NodeIndex> {
        let label = concept.label.clone();
        let vector = match vector {
            Some(raw) => {
                let padded_dim = self
                    .dimensions
                    .register_or_check(raw.len())
                    .map_err(|e| anyhow::anyhow!("{}", e))?;
                Some(align_vector(raw, padded_dim))
            }
            None => None,
        };

        let mut g = self.graph.write();
        let idx = g.add_node(concept.clone());
        self.index.insert(label.clone(), idx);

        if let Some(ref vec) = vector {
            self.vectors
                .insert(label.clone(), vec.clone())
                .map_err(|e| anyhow::anyhow!("{}", e))?;
        }

        // Persist concept
        let val = bincode::serialize(&concept).context("serializing concept")?;
        let key = format!("concept:{}", idx.index());
        self.db.insert(key.as_bytes(), val)?;

        Ok(idx)
    }

    /// Lookup a concept by label.
    pub fn get(&self, label: &str) -> Option<Concept> {
        let idx = *self.index.get(label)?;
        let g = self.graph.read();
        g.node_weight(idx).cloned()
    }

    /// Create an undirected edge between two concepts.
    pub fn link(&self, a: &str, b: &str, weight: f32) -> Result<()> {
        let idx_a = self
            .index
            .get(a)
            .map(|i| *i)
            .context(format!("concept '{}' not found", a))?;
        let idx_b = self
            .index
            .get(b)
            .map(|i| *i)
            .context(format!("concept '{}' not found", b))?;

        let mut g = self.graph.write();
        g.add_edge(idx_a, idx_b, weight);
        drop(g);

        let key = if idx_a.index() < idx_b.index() {
            (idx_a.index(), idx_b.index())
        } else {
            (idx_b.index(), idx_a.index())
        };
        self.edge_timestamps.insert(key, Utc::now());

        // Persist edge
        let db_key = format!("edge:{}:{}", key.0, key.1);
        let record = EdgeRecord {
            a: key.0,
            b: key.1,
            weight,
        };
        let val = bincode::serialize(&record).context("serializing edge")?;
        self.db.insert(db_key.as_bytes(), val)?;

        Ok(())
    }

    /// Semantic search via VectorStore.
    pub fn search(&self, query: &[f32], top_k: usize) -> Vec<SearchResult> {
        self.vectors.search(query, top_k)
    }

    /// Update the importance (weight) of a concept's edges.
    /// Applies a gradient-like adjustment to all edges connected to the concept.
    /// Returns true if any edge was updated, false if concept not found.
    /// Changes are persisted to sled on the next meditation cycle.
    pub fn update_importance(&self, label: &str, adjustment: f32) -> Result<bool> {
        let idx = match self.index.get(label) {
            Some(i) => *i,
            None => return Ok(false),
        };

        let mut g = self.graph.write();
        let edge_indices: Vec<_> = g.edges(idx).map(|e| e.id()).collect();

        let mut updated = false;
        for eidx in edge_indices {
            if let Some(w) = g.edge_weight_mut(eidx) {
                let new_weight = (*w + adjustment).clamp(0.01, 10.0);
                *w = new_weight;
                updated = true;
            }
        }

        // Also touch the concept node
        if let Some(node) = g.node_weight_mut(idx) {
            node.touch();
        }

        Ok(updated)
    }

    /// Update a specific edge weight between two concepts.
    /// Persists to sled immediately.
    pub fn update_edge(&self, a: &str, b: &str, adjustment: f32) -> Result<bool> {
        let idx_a = match self.index.get(a) {
            Some(i) => *i,
            None => return Ok(false),
        };
        let idx_b = match self.index.get(b) {
            Some(i) => *i,
            None => return Ok(false),
        };

        let mut g = self.graph.write();
        let edge_id = g.edges_connecting(idx_a, idx_b).next().map(|e| e.id());
        if let Some(eid) = edge_id {
            if let Some(w) = g.edge_weight_mut(eid) {
                *w = (*w + adjustment).clamp(0.01, 10.0);
            }
        }
        drop(g);

        // Persist edge to sled
        let key = format!("edge:{}:{}", a, b);
        let record = EdgeRecord {
            a: idx_a.index(),
            b: idx_b.index(),
            weight: adjustment, // the adjustment applied
        };
        let val = serde_json::to_vec(&record)?;
        self.db.insert(key.as_bytes(), val)?;

        Ok(true)
    }

    /// Get neighbors of a concept in the graph.
    pub fn neighbors(&self, label: &str) -> Vec<(Concept, f32)> {
        let idx = match self.index.get(label) {
            Some(i) => *i,
            None => return Vec::new(),
        };
        let g = self.graph.read();
        let mut result = Vec::new();
        let edges: Vec<_> = g
            .edges_directed(idx, petgraph::Direction::Outgoing)
            .map(|e| (e.target(), *e.weight()))
            .collect();
        for (neighbor_idx, weight) in edges {
            if let Some(concept) = g.node_weight(neighbor_idx) {
                result.push((concept.clone(), weight));
            }
        }
        result
    }

    /// Apply temporal decay to all edge weights.
    pub fn decay_all(&self) -> Result<()> {
        let now = Utc::now();
        let mut g = self.graph.write();
        let edge_indices: Vec<_> = g.edge_indices().collect();

        for eidx in edge_indices {
            let (a, b) = match g.edge_endpoints(eidx) {
                Some(ep) => ep,
                None => continue,
            };
            let key = {
                let ai = a.index();
                let bi = b.index();
                if ai < bi {
                    (ai, bi)
                } else {
                    (bi, ai)
                }
            };

            let age = match self.edge_timestamps.get(&key) {
                Some(ts) => (now - *ts).num_milliseconds() as f64 / 1000.0,
                None => 0.0,
            };

            let current_weight = g.edge_weight(eidx).copied().unwrap_or(0.0);
            match self.decay.apply(current_weight, age) {
                Some(new_weight) => {
                    *g.edge_weight_mut(eidx).unwrap() = new_weight;
                }
                None => {
                    g.remove_edge(eidx);
                    self.edge_timestamps.remove(&key);
                }
            }
        }
        Ok(())
    }

    /// Flush sled to disk.
    pub fn persist(&self) -> Result<()> {
        self.db.flush()?;
        Ok(())
    }

    /// Rebuild graph from sled on startup.
    pub fn load_from_db(&mut self) -> Result<()> {
        let (concepts, edges) = persistence::load_from_db(&self.db)?;

        // Index mapping: persisted index → new NodeIndex
        let mut idx_map = std::collections::HashMap::new();
        let mut g = self.graph.write();

        for (old_idx, concept) in &concepts {
            let new_idx = g.add_node(concept.clone());
            self.index.insert(concept.label.clone(), new_idx);
            idx_map.insert(*old_idx, new_idx);

            // Re-insert vector if we had one
            if self.dimensions.padded_dim().is_some() {
                // We don't store vectors in sled (they're in VectorStore which is in-memory)
                // Vectors need to be re-added via insert after load
            }
        }

        for record in &edges {
            if let (Some(&a), Some(&b)) = (idx_map.get(&record.a), idx_map.get(&record.b)) {
                g.add_edge(a, b, record.weight);
                let key = if record.a < record.b {
                    (record.a, record.b)
                } else {
                    (record.b, record.a)
                };
                self.edge_timestamps.insert(key, Utc::now()); // approximate
            }
        }

        Ok(())
    }

    /// Number of concepts in the graph.
    pub fn len(&self) -> usize {
        self.graph.read().node_count()
    }

    /// Whether the graph is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ConceptKind, DecayConfig};
    use std::time::Duration;
    use tempfile::TempDir;

    fn make_graph(dir: &Path) -> MemoryGraph {
        MemoryGraph::open(dir, DecayConfig::default()).unwrap()
    }

    #[test]
    fn insert_and_get() {
        let dir = TempDir::new().unwrap();
        let g = make_graph(dir.path());
        let c = Concept::new("rust", ConceptKind::Skill);
        g.insert(c.clone(), None).unwrap();
        let got = g.get("rust").unwrap();
        assert_eq!(got.label, "rust");
        assert_eq!(got.kind, ConceptKind::Skill);
    }

    #[test]
    fn link_and_neighbors() {
        let dir = TempDir::new().unwrap();
        let g = make_graph(dir.path());
        g.insert(Concept::new("a", ConceptKind::Fact), None)
            .unwrap();
        g.insert(Concept::new("b", ConceptKind::Fact), None)
            .unwrap();
        g.link("a", "b", 0.8).unwrap();
        let nbrs = g.neighbors("a");
        assert_eq!(nbrs.len(), 1);
        assert_eq!(nbrs[0].0.label, "b");
        assert!((nbrs[0].1 - 0.8).abs() < 1e-6);
    }

    #[test]
    fn search_with_vectors() {
        let dir = TempDir::new().unwrap();
        let g = make_graph(dir.path());

        // Insert 3 concepts with simple vectors
        let v1 = vec![1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        let v2 = vec![0.9, 0.1, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        let v3 = vec![0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        g.insert(Concept::new("cat", ConceptKind::Fact), Some(v1))
            .unwrap();
        g.insert(Concept::new("kitten", ConceptKind::Fact), Some(v2))
            .unwrap();
        g.insert(Concept::new("car", ConceptKind::Fact), Some(v3))
            .unwrap();

        let query = vec![1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        let results = g.search(&query, 2);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].label, "cat");
    }

    #[test]
    fn decay_applied_to_edges() {
        let mut cfg = DecayConfig::default();
        cfg.half_life_secs = 1.0; // 1 second half-life for testing
        let dir = TempDir::new().unwrap();
        let g = MemoryGraph::open(dir.path(), cfg).unwrap();

        g.insert(Concept::new("a", ConceptKind::Fact), None)
            .unwrap();
        g.insert(Concept::new("b", ConceptKind::Fact), None)
            .unwrap();
        g.link("a", "b", 1.0).unwrap();

        // Wait for decay (500ms with 1s half-life → factor ~0.7)
        std::thread::sleep(Duration::from_millis(500));
        g.decay_all().unwrap();

        let nbrs = g.neighbors("a");
        // Edge should still exist but with reduced weight
        assert_eq!(nbrs.len(), 1);
        assert!(nbrs[0].1 < 1.0);
    }

    #[test]
    fn dimension_mismatch_rejected() {
        let dir = TempDir::new().unwrap();
        let g = make_graph(dir.path());

        let v1 = vec![0.0f32; 8];
        g.insert(Concept::new("first", ConceptKind::Fact), Some(v1))
            .unwrap();

        // Wrong dimension should fail
        let v_bad = vec![0.0f32; 16];
        let result = g.insert(Concept::new("bad", ConceptKind::Fact), Some(v_bad));
        assert!(result.is_err());
    }

    #[test]
    fn persistence_roundtrip() {
        let dir = TempDir::new().unwrap();

        // Create and populate
        {
            let g = MemoryGraph::open(dir.path(), DecayConfig::default()).unwrap();
            g.insert(Concept::new("rust", ConceptKind::Skill), None)
                .unwrap();
            g.insert(Concept::new("memory", ConceptKind::Fact), None)
                .unwrap();
            g.link("rust", "memory", 0.75).unwrap();
            g.persist().unwrap();
        }

        // Reload
        {
            let g2 = MemoryGraph::open(dir.path(), DecayConfig::default()).unwrap();
            let c = g2.get("rust").unwrap();
            assert_eq!(c.label, "rust");
            let nbrs = g2.neighbors("rust");
            assert_eq!(nbrs.len(), 1);
            assert_eq!(nbrs[0].0.label, "memory");
        }
    }

    #[test]
    fn get_missing_returns_none() {
        let dir = TempDir::new().unwrap();
        let g = make_graph(dir.path());
        assert!(g.get("nonexistent").is_none());
    }

    #[test]
    fn link_missing_concept_fails() {
        let dir = TempDir::new().unwrap();
        let g = make_graph(dir.path());
        assert!(g.link("a", "b", 0.5).is_err());
    }

    #[test]
    fn update_importance_adjusts_edges() {
        let dir = TempDir::new().unwrap();
        let g = make_graph(dir.path());
        g.insert(Concept::new("cat", ConceptKind::Fact), None)
            .unwrap();
        g.insert(Concept::new("kitten", ConceptKind::Fact), None)
            .unwrap();
        g.link("cat", "kitten", 0.5).unwrap();

        let nbrs_before = g.neighbors("cat");
        let weight_before = nbrs_before[0].1;

        g.update_importance("cat", 0.3).unwrap();

        let nbrs_after = g.neighbors("cat");
        let weight_after = nbrs_after[0].1;
        assert!(
            weight_after > weight_before,
            "weight should increase: {} -> {}",
            weight_before,
            weight_after
        );
    }
}
