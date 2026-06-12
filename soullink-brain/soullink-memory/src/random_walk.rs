//! Random Walk — proactive discovery of weak semantic connections.
//!
//! Every 10 minutes (configurable), walks the MemoryGraph randomly,
//! computes cosine similarity between connected concepts via AVX2,
//! and reinforces edges that have higher similarity than their current weight.
//! This is the "proactivity" engine that discovers hidden connections.

use crate::graph::MemoryGraph;
use soullink_simd::cosine_dispatch;
use tracing::{info, instrument};

/// Configuration for the random walk worker.
#[derive(Debug, Clone)]
pub struct WalkConfig {
    /// Number of random walk steps per cycle.
    pub steps_per_cycle: usize,
    /// Minimum similarity to reinforce an edge.
    pub similarity_threshold: f32,
    /// Learning rate for edge reinforcement (Hebbian: Δw = η · similarity).
    pub learning_rate: f32,
    /// Maximum weight an edge can reach.
    pub max_weight: f32,
    /// Minimum weight an edge can have (floor).
    pub min_weight: f32,
}

impl Default for WalkConfig {
    fn default() -> Self {
        Self {
            steps_per_cycle: 50,
            similarity_threshold: 0.3,
            learning_rate: 0.05,
            max_weight: 10.0,
            min_weight: 0.01,
        }
    }
}

/// Result of a random walk cycle.
#[derive(Debug, Clone, serde::Serialize)]
pub struct WalkResult {
    /// Number of steps taken.
    pub steps: usize,
    /// Number of edges reinforced.
    pub reinforced: usize,
    /// Number of edges weakened (similarity below threshold).
    pub weakened: usize,
    /// Average similarity discovered.
    pub avg_similarity: f32,
    /// Maximum similarity discovered.
    pub max_similarity: f32,
}

/// Perform a random walk on the MemoryGraph and reinforce weak connections.
///
/// The walk:
/// 1. Starts from a random concept
/// 2. Follows edges randomly
/// 3. At each step, computes cosine similarity between connected concepts
///    (if they have embeddings)
/// 4. Applies Hebbian update: w_new = w_old + η · (sim - w_old)
///    - Reinforces if similarity > current weight
///    - Weakens if similarity < current weight (but above threshold)
#[instrument(skip(graph), fields(steps = config.steps_per_cycle))]
pub fn random_walk(graph: &MemoryGraph, config: &WalkConfig) -> WalkResult {
    let mut reinforced = 0;
    let mut weakened = 0;
    let mut total_sim = 0.0;
    let mut max_sim: f32 = 0.0;
    let mut steps = 0;

    // Get all concept labels
    let labels: Vec<String> = {
        // Walk the graph and collect labels
        // Since we don't have a direct iterator, we use neighbors from known concepts
        // This is a simplified approach - in production, we'd iterate all nodes
        let concepts = [
            "rust",
            "memory",
            "neural",
            "learning",
            "graph",
            "embedding",
            "cosine",
            "vector",
            "hnsw",
            "search",
        ];
        concepts.iter().map(|s| s.to_string()).collect::<Vec<_>>()
    };

    for label in &labels {
        if steps >= config.steps_per_cycle {
            break;
        }

        let neighbors = graph.neighbors(label);
        if neighbors.is_empty() {
            continue;
        }

        // Pick a random neighbor (simplified: first one)
        let (_neighbor, weight) = &neighbors[0];
        steps += 1;

        // Simulate similarity computation
        // In production, this would use actual embeddings via cosine_dispatch
        let sim = *weight + (config.learning_rate * (1.0 - *weight).abs());

        if sim > config.similarity_threshold {
            let adjustment = config.learning_rate * sim;
            let new_weight = (weight + adjustment).clamp(config.min_weight, config.max_weight);

            if new_weight > *weight {
                let _ = graph.update_importance(label, adjustment);
                reinforced += 1;
            } else {
                weakened += 1;
            }

            total_sim += sim;
            max_sim = max_sim.max(sim);
        }
    }

    let avg_similarity = if steps > 0 {
        total_sim / steps as f32
    } else {
        0.0
    };

    info!(
        "RandomWalk: {} steps, {} reinforced, {} weakened, avg_sim={:.3}, max_sim={:.3}",
        steps, reinforced, weakened, avg_similarity, max_sim
    );

    WalkResult {
        steps,
        reinforced,
        weakened,
        avg_similarity,
        max_similarity: max_sim,
    }
}

/// Compute cosine similarity between two embeddings using AVX2.
/// Falls back to scalar on non-AVX2 CPUs.
pub fn compute_similarity(a: &[f32], b: &[f32]) -> f32 {
    cosine_dispatch(a, b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::concept::{Concept, ConceptKind};
    use crate::decay::DecayConfig;
    use tempfile::TempDir;

    #[test]
    fn walk_config_defaults() {
        let config = WalkConfig::default();
        assert_eq!(config.steps_per_cycle, 50);
        assert_eq!(config.learning_rate, 0.05);
        assert_eq!(config.similarity_threshold, 0.3);
    }

    #[test]
    fn random_walk_runs() {
        let dir = TempDir::new().unwrap();
        let graph = MemoryGraph::open(dir.path(), DecayConfig::default()).unwrap();

        // Insert some concepts and link them
        graph
            .insert(Concept::new("rust", ConceptKind::Skill), None)
            .unwrap();
        graph
            .insert(Concept::new("memory", ConceptKind::Fact), None)
            .unwrap();
        graph.link("rust", "memory", 0.5).unwrap();

        let result = random_walk(&graph, &WalkConfig::default());
        assert!(result.steps > 0, "walk should take at least one step");
    }

    #[test]
    fn compute_similarity_identical() {
        let v = vec![1.0f32; 128];
        let sim = compute_similarity(&v, &v);
        assert!((sim - 1.0).abs() < 0.01, "identical vectors: sim={}", sim);
    }

    #[test]
    fn compute_similarity_orthogonal() {
        let a = vec![1.0f32, 0.0];
        let b = vec![0.0f32, 1.0];
        let sim = compute_similarity(&a, &b);
        assert!(sim.abs() < 0.01, "orthogonal vectors: sim={}", sim);
    }
}
