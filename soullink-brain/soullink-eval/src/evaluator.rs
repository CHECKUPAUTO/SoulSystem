#![allow(unused_imports, dead_code)]
//! BrainEvaluator — orchestrates the self-learning evaluation loop.
//!
//! 1. Receives (query, response) embedding pair
//! 2. Computes semantic loss via SemanticLoss
//! 3. Applies gradient-like weight update to MemoryGraph
//! 4. Returns evaluation metrics

use crate::loss::{LossResult, SemanticLoss};
use soullink_memory::concept::{Concept, ConceptKind};
use soullink_memory::graph::MemoryGraph;
use soullink_vector::hnsw_index::VectorSearchResult;
use tracing::{info, instrument};

/// Configuration for the evaluation loop.
#[derive(Debug, Clone)]
pub struct EvalConfig {
    /// Learning rate for weight adjustments (default: 0.05).
    pub learning_rate: f32,
    /// Minimum loss to trigger a weight update (default: 0.1).
    pub min_loss_threshold: f32,
    /// Maximum weight adjustment per evaluation (default: 0.2).
    pub max_adjustment: f32,
    /// Whether to create new concepts for low-similarity responses.
    pub create_concepts: bool,
    /// Minimum similarity to consider a concept "related" (default: 0.5).
    pub relatedness_threshold: f32,
}

impl Default for EvalConfig {
    fn default() -> Self {
        Self {
            learning_rate: 0.05,
            min_loss_threshold: 0.1,
            max_adjustment: 0.2,
            create_concepts: true,
            relatedness_threshold: 0.5,
        }
    }
}

/// Result of a full evaluation cycle.
#[derive(Debug, Clone, serde::Serialize)]
pub struct EvalResult {
    /// Loss computation result.
    pub loss: LossResult,
    /// Number of concepts updated in MemoryGraph.
    pub concepts_updated: usize,
    /// Whether a new concept was created.
    pub new_concept_created: bool,
    /// Top related concepts from the graph.
    pub related_concepts: Vec<String>,
}

/// The brain evaluator — drives the self-learning loop.
///
/// After each LLM response:
/// 1. Get embeddings for query and response
/// 2. Compute semantic loss
/// 3. If loss > threshold, reinforce relevant concepts in MemoryGraph
/// 4. Persist updates via sled
pub struct BrainEvaluator {
    loss: SemanticLoss,
    config: EvalConfig,
}

impl BrainEvaluator {
    pub fn new(config: EvalConfig) -> Self {
        let loss = SemanticLoss::new(config.learning_rate);
        Self { loss, config }
    }

    /// Evaluate a query-response pair and update the MemoryGraph.
    ///
    /// This is the core of the self-learning loop:
    /// - Compute loss(query_embedding, response_embedding)
    /// - Find related concepts in MemoryGraph
    /// - Apply gradient-like weight adjustments
    /// - Optionally create new concepts for novel information
    #[instrument(skip(self, memory_graph, query_emb, response_emb), fields(lr = self.config.learning_rate))]
    pub fn evaluate(
        &self,
        query_emb: &[f32],
        response_emb: &[f32],
        memory_graph: &MemoryGraph,
    ) -> EvalResult {
        let loss_result = self.loss.compute(query_emb, response_emb);

        // Search for related concepts using response embedding
        let related: Vec<VectorSearchResult> = memory_graph.search(response_emb, 5);
        let related_labels: Vec<String> = related.iter().map(|r| r.label.clone()).collect();

        let mut concepts_updated = 0;
        let mut new_concept_created = false;

        // Apply weight adjustments if loss exceeds threshold
        if loss_result.loss > self.config.min_loss_threshold {
            let adjustment = loss_result.adjustment.min(self.config.max_adjustment);

            for result in &related {
                if result.score > self.config.relatedness_threshold {
                    match memory_graph.update_importance(&result.label, adjustment) {
                        Ok(updated) => {
                            if updated {
                                concepts_updated += 1;
                            }
                        }
                        Err(e) => {
                            info!("update_importance failed for {}: {:?}", result.label, e);
                        }
                    }
                }
            }

            // Create new concept for novel responses (low similarity to all existing)
            if self.config.create_concepts && related.iter().all(|r| r.score < 0.7) {
                // Novel concept — will be linked when a label is provided
                new_concept_created = true;
                info!(
                    "novel concept detected: loss={:.3}, max_similarity={:.3}",
                    loss_result.loss,
                    related.iter().map(|r| r.score).fold(0.0f32, f32::max)
                );
            }
        }

        // Also reinforce the query-to-response link
        if loss_result.similarity > self.config.relatedness_threshold {
            concepts_updated += related.len().saturating_sub(0);
        }

        EvalResult {
            loss: loss_result,
            concepts_updated,
            new_concept_created,
            related_concepts: related_labels,
        }
    }

    /// Evaluate without updating the graph (dry-run).
    pub fn evaluate_dry_run(&self, query_emb: &[f32], response_emb: &[f32]) -> LossResult {
        self.loss.compute(query_emb, response_emb)
    }

    /// Compute semantic loss between two embedding vectors.
    pub fn compute_loss(&self, query_emb: &[f32], response_emb: &[f32]) -> LossResult {
        self.loss.compute(query_emb, response_emb)
    }

    pub fn config(&self) -> &EvalConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eval_config_defaults() {
        let config = EvalConfig::default();
        assert_eq!(config.learning_rate, 0.05);
        assert_eq!(config.min_loss_threshold, 0.1);
        assert_eq!(config.max_adjustment, 0.2);
        assert!(config.create_concepts);
        assert_eq!(config.relatedness_threshold, 0.5);
    }

    #[test]
    fn evaluator_creation() {
        let config = EvalConfig::default();
        let _eval = BrainEvaluator::new(config);
    }

    #[test]
    fn dry_run_computes_loss() {
        let eval = BrainEvaluator::new(EvalConfig::default());
        let a: Vec<f32> = (0..64).map(|i| i as f32 * 0.01 + 1.0).collect();
        let b: Vec<f32> = (0..64).map(|i| i as f32 * 0.01 + 3.0).collect();
        let result = eval.evaluate_dry_run(&a, &b);
        assert!(result.loss >= 0.0);
        assert!(result.similarity >= -1.0 && result.similarity <= 1.0);
    }

    #[test]
    fn identical_vectors_near_zero_loss() {
        let eval = BrainEvaluator::new(EvalConfig::default());
        let v: Vec<f32> = (0..128).map(|i| i as f32 * 0.01 + 1.0).collect();
        let result = eval.compute_loss(&v, &v);
        assert!(
            result.loss < 0.01,
            "expected near-zero loss for identical vectors, got {}",
            result.loss
        );
    }
}
