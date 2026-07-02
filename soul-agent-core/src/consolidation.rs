//! Vector-Based Memory Consolidation
//!
//! Replaces Jaccard similarity with embedding-based semantic clustering
//! for memory consolidation. Uses cosine similarity on vector embeddings
//! to group related memories and consolidate them into higher-level concepts.
//!
//! ## Pipeline:
//! 1. Generate embeddings for episodic memories (via LLM or local encoder)
//! 2. Cluster memories by embedding similarity
//! 3. For each cluster, generate a consolidated semantic summary
//! 4. Store consolidated memories in semantic layer
//! 5. Decay original episodic memories

use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

/// Configuration for vector-based consolidation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorConsolidationConfig {
    /// Embedding dimension (e.g., 384 for all-MiniLM-L6-v2).
    pub embedding_dim: usize,
    /// Cosine similarity threshold for clustering (0.0-1.0).
    pub similarity_threshold: f32,
    /// Minimum cluster size to trigger consolidation.
    pub min_cluster_size: usize,
    /// Maximum clusters per consolidation run.
    pub max_clusters: usize,
    /// Decay factor for original episodic memories after consolidation.
    pub decay_factor: f32,
    /// Whether to generate summaries for clusters.
    pub generate_summaries: bool,
}

impl Default for VectorConsolidationConfig {
    fn default() -> Self {
        Self {
            embedding_dim: 384,
            similarity_threshold: 0.75,
            min_cluster_size: 3,
            max_clusters: 10,
            decay_factor: 0.5,
            generate_summaries: true,
        }
    }
}

/// A memory entry with its vector embedding.
#[derive(Debug, Clone)]
pub struct VectorMemory {
    pub id: String,
    pub text: String,
    pub embedding: Vec<f32>,
    pub importance: f32,
    pub access_count: usize,
}

/// A cluster of related memories.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryCluster {
    pub id: String,
    pub members: Vec<String>,
    pub centroid: Vec<f32>,
    pub cohesion_score: f32,
    pub summary: Option<String>,
    pub dominant_theme: Option<String>,
}

/// Result of a consolidation run.
#[derive(Debug, Clone)]
pub struct ConsolidationResult {
    /// Number of clusters formed.
    pub clusters_formed: usize,
    /// Total memories consolidated.
    pub memories_consolidated: usize,
    /// Average cluster cohesion score.
    pub avg_cohesion: f32,
    /// Number of clusters above cohesion threshold.
    pub high_quality_clusters: usize,
}

/// Vector consolidation engine.
pub struct VectorConsolidator {
    config: VectorConsolidationConfig,
    /// Buffer of episodic vectors awaiting consolidation.
    buffer: Vec<VectorMemory>,
    /// Formed clusters.
    clusters: Vec<MemoryCluster>,
}

impl VectorConsolidator {
    pub fn new(config: VectorConsolidationConfig) -> Self {
        Self {
            config,
            buffer: Vec::new(),
            clusters: Vec::new(),
        }
    }

    /// Add a memory to the consolidation buffer.
    pub fn add_memory(&mut self, memory: VectorMemory) {
        self.buffer.push(memory);
    }

    /// Compute cosine similarity between two vectors.
    pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() || a.is_empty() {
            return 0.0;
        }

        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

        if norm_a == 0.0 || norm_b == 0.0 {
            return 0.0;
        }

        dot / (norm_a * norm_b)
    }

    /// Compute the centroid (mean vector) of a set of vectors.
    pub fn centroid(embeddings: &[&[f32]], dim: usize) -> Vec<f32> {
        if embeddings.is_empty() {
            return vec![0.0; dim];
        }

        let mut centroid = vec![0.0f32; dim];
        for emb in embeddings {
            for (i, &val) in emb.iter().enumerate() {
                if i < dim {
                    centroid[i] += val;
                }
            }
        }

        let n = embeddings.len() as f32;
        for val in centroid.iter_mut() {
            *val /= n;
        }

        centroid
    }

    /// Cluster memories using cosine similarity with the given threshold.
    /// Returns clusters of memory indices.
    pub fn cluster_memories(&self, memories: &[VectorMemory], threshold: f32) -> Vec<Vec<usize>> {
        if memories.is_empty() {
            return Vec::new();
        }

        let n = memories.len();
        let mut visited = vec![false; n];
        let mut clusters = Vec::new();

        // Build similarity matrix (symmetric)
        let mut similarity = vec![vec![0.0f32; n]; n];
        for i in 0..n {
            for j in i..n {
                let sim = if i == j {
                    1.0
                } else {
                    Self::cosine_similarity(&memories[i].embedding, &memories[j].embedding)
                };
                similarity[i][j] = sim;
                similarity[j][i] = sim;
            }
        }

        // Greedy clustering
        for i in 0..n {
            if visited[i] {
                continue;
            }

            let mut cluster = vec![i];
            visited[i] = true;

            for j in (i + 1)..n {
                if visited[j] {
                    continue;
                }

                // Check if this memory is similar to ALL members of the cluster
                let all_similar = cluster.iter().all(|&k| similarity[k][j] >= threshold);
                if all_similar {
                    cluster.push(j);
                    visited[j] = true;
                }
            }

            if cluster.len() >= self.config.min_cluster_size {
                clusters.push(cluster);
            }
        }

        // Limit max clusters
        if clusters.len() > self.config.max_clusters {
            // Sort by cluster size (largest first) and take top K
            clusters.sort_by(|a, b| b.len().cmp(&a.len()));
            clusters.truncate(self.config.max_clusters);
        }

        clusters
    }

    /// Form clusters from current buffer and compute centroids.
    pub fn form_clusters(&mut self) -> Vec<MemoryCluster> {
        let cluster_indices = self.cluster_memories(&self.buffer, self.config.similarity_threshold);
        let mut clusters = Vec::new();

        for (cluster_id, indices) in cluster_indices.iter().enumerate() {
            let members: Vec<String> = indices.iter().map(|&i| self.buffer[i].id.clone()).collect();

            let embeddings: Vec<&[f32]> = indices
                .iter()
                .map(|&i| self.buffer[i].embedding.as_slice())
                .collect();

            let centroid = Self::centroid(&embeddings, self.config.embedding_dim);

            // Calculate cohesion: average pairwise similarity within cluster
            let mut cohesion_sum = 0.0f32;
            let mut pairs = 0;
            for i in 0..indices.len() {
                for j in (i + 1)..indices.len() {
                    cohesion_sum += Self::cosine_similarity(
                        &self.buffer[indices[i]].embedding,
                        &self.buffer[indices[j]].embedding,
                    );
                    pairs += 1;
                }
            }
            let cohesion = if pairs > 0 {
                cohesion_sum / pairs as f32
            } else {
                1.0
            };

            // Detect dominant theme from member texts
            let theme = self.detect_theme(
                &indices
                    .iter()
                    .map(|&i| &self.buffer[i].text)
                    .collect::<Vec<_>>(),
            );

            clusters.push(MemoryCluster {
                id: format!("cluster_{}", cluster_id),
                members,
                centroid,
                cohesion_score: cohesion,
                summary: None,
                dominant_theme: theme,
            });
        }

        self.clusters = clusters.clone();
        clusters
    }

    /// Simple theme detection from cluster member texts.
    fn detect_theme(&self, texts: &[&String]) -> Option<String> {
        if texts.is_empty() {
            return None;
        }

        // Count word frequency across all texts
        let all_text: String = texts
            .iter()
            .map(|t| t.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let words: Vec<&str> = all_text
            .split_whitespace()
            .filter(|w| w.len() > 3)
            .collect();

        let mut freq: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        for word in &words {
            *freq.entry(word).or_insert(0) += 1;
        }

        // Find most common meaningful word
        let stopwords = [
            "this", "that", "with", "from", "have", "been", "were", "they", "will",
        ];

        let mut top_word: Option<(&str, usize)> = None;
        for (word, count) in freq.iter() {
            let lower = word.to_lowercase();
            if stopwords.iter().any(|s| s == &lower) {
                continue;
            }
            if top_word.map_or(true, |(_, c)| *count > c) {
                top_word = Some((word, *count));
            }
        }

        top_word.map(|(w, _)| w.to_string())
    }

    /// Run consolidation on the current buffer.
    pub fn consolidate(&mut self) -> ConsolidationResult {
        let initial_count = self.buffer.len();

        if initial_count < self.config.min_cluster_size {
            return ConsolidationResult {
                clusters_formed: 0,
                memories_consolidated: 0,
                avg_cohesion: 0.0,
                high_quality_clusters: 0,
            };
        }

        let clusters = self.form_clusters();
        let cluster_count = clusters.len();

        let total_consolidated: usize = clusters.iter().map(|c| c.members.len()).sum();
        let avg_cohesion: f32 = if !clusters.is_empty() {
            clusters.iter().map(|c| c.cohesion_score).sum::<f32>() / clusters.len() as f32
        } else {
            0.0
        };
        let high_quality = clusters
            .iter()
            .filter(|c| c.cohesion_score >= self.config.similarity_threshold)
            .count();

        // Decay original episodic memories in consolidated clusters
        let consolidated_ids: std::collections::HashSet<String> =
            clusters.iter().flat_map(|c| c.members.clone()).collect();
        self.buffer.retain(|m| !consolidated_ids.contains(&m.id));

        ConsolidationResult {
            clusters_formed: cluster_count,
            memories_consolidated: total_consolidated,
            avg_cohesion,
            high_quality_clusters: high_quality,
        }
    }

    /// Get current clusters.
    pub fn get_clusters(&self) -> &[MemoryCluster] {
        &self.clusters
    }

    /// Buffer size.
    pub fn buffer_size(&self) -> usize {
        self.buffer.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_similarity_identical() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((VectorConsolidator::cosine_similarity(&a, &b) - 1.0).abs() < 0.001);
    }

    #[test]
    fn cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        assert!((VectorConsolidator::cosine_similarity(&a, &b) - 0.0).abs() < 0.001);
    }

    #[test]
    fn cosine_similarity_opposite() {
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        assert!((VectorConsolidator::cosine_similarity(&a, &b) - (-1.0)).abs() < 0.001);
    }

    #[test]
    fn centroid_computation() {
        let a = vec![1.0, 1.0];
        let b = vec![3.0, 3.0];
        let centroid = VectorConsolidator::centroid(&[&a[..], &b[..]], 2);
        assert!((centroid[0] - 2.0).abs() < 0.001);
        assert!((centroid[1] - 2.0).abs() < 0.001);
    }

    #[test]
    fn cluster_memories_groups_similar() {
        let config = VectorConsolidationConfig {
            min_cluster_size: 2,
            ..Default::default()
        };
        let consolidator = VectorConsolidator::new(config);

        let memories = vec![
            VectorMemory {
                id: "m1".into(),
                text: "rust programming language".into(),
                embedding: vec![1.0, 0.1, 0.0],
                importance: 0.8,
                access_count: 1,
            },
            VectorMemory {
                id: "m2".into(),
                text: "rust compiler and cargo".into(),
                embedding: vec![0.9, 0.2, 0.0],
                importance: 0.7,
                access_count: 2,
            },
            VectorMemory {
                id: "m3".into(),
                text: "python script for data".into(),
                embedding: vec![0.0, 1.0, 1.0],
                importance: 0.6,
                access_count: 3,
            },
        ];

        let clusters = consolidator.cluster_memories(&memories, 0.7);
        assert!(clusters.len() >= 1);
        // m1 and m2 should be in same cluster (similar)
        let has_m1_m2 = clusters.iter().any(|c| c.contains(&0) && c.contains(&1));
        assert!(has_m1_m2);
    }

    #[test]
    fn consolidate_full_pipeline() {
        let config = VectorConsolidationConfig {
            min_cluster_size: 2,
            embedding_dim: 3,
            similarity_threshold: 0.7,
            max_clusters: 5,
            decay_factor: 0.5,
            generate_summaries: false,
        };
        let mut consolidator = VectorConsolidator::new(config);

        consolidator.add_memory(VectorMemory {
            id: "a".into(),
            text: "rust".into(),
            embedding: vec![1.0, 0.0, 0.0],
            importance: 0.8,
            access_count: 1,
        });
        consolidator.add_memory(VectorMemory {
            id: "b".into(),
            text: "rust".into(),
            embedding: vec![0.95, 0.05, 0.0],
            importance: 0.7,
            access_count: 2,
        });

        let result = consolidator.consolidate();
        assert!(result.clusters_formed >= 1);
        assert!(result.memories_consolidated >= 2);
    }
}
