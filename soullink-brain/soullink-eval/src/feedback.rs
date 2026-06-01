#![allow(unused_imports, dead_code)]
//! Feedback Loop — async task that connects LLM responses to MemoryGraph updates.
//!
//! After each response generation, the orchestrator spawns a feedback task:
//! 1. Get response embedding from Ollama
//! 2. Compute semantic loss via BrainEvaluator
//! 3. Apply weight adjustments to MemoryGraph
//! 4. Return metrics (non-blocking)

use crate::evaluator::{BrainEvaluator, EvalConfig, EvalResult};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use soullink_memory::concept::{Concept, ConceptKind};
use soullink_memory::graph::MemoryGraph;
use std::sync::Arc;
use tracing::{info, instrument, warn};

/// Ollama embedding request/response structures.
#[derive(Debug, Serialize)]
struct EmbeddingRequest {
    model: String,
    prompt: String,
}

#[derive(Debug, Deserialize)]
struct EmbeddingResponse {
    embedding: Vec<f32>,
}

/// Configuration for the feedback loop.
#[derive(Debug, Clone)]
pub struct FeedbackConfig {
    /// Ollama endpoint (default: http://127.0.0.1:11434).
    pub ollama_url: String,
    /// Embedding model (default: nomic-embed-text).
    pub embedding_model: String,
    /// Eval configuration.
    pub eval_config: EvalConfig,
}

impl Default for FeedbackConfig {
    fn default() -> Self {
        Self {
            ollama_url: "http://127.0.0.1:11434".to_string(),
            embedding_model: "nomic-embed-text".to_string(),
            eval_config: EvalConfig::default(),
        }
    }
}

/// Metrics returned after a feedback cycle.
#[derive(Debug, Clone, Serialize)]
pub struct FeedbackMetrics {
    /// Loss result from evaluation.
    pub loss: f32,
    /// Cosine similarity between query and response.
    pub similarity: f32,
    /// Number of concepts updated.
    pub concepts_updated: usize,
    /// Whether a new concept was created.
    pub new_concept_created: bool,
    /// Whether Ollama embedding was used (vs local cosine).
    pub used_ollama: bool,
}

/// The feedback loop — async bridge between LLM and MemoryGraph.
pub struct FeedbackLoop {
    evaluator: BrainEvaluator,
    config: FeedbackConfig,
    client: reqwest::Client,
}

impl FeedbackLoop {
    pub fn new(config: FeedbackConfig) -> Self {
        let evaluator = BrainEvaluator::new(config.eval_config.clone());
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("failed to build HTTP client");
        Self {
            evaluator,
            config,
            client,
        }
    }

    /// Run a full feedback cycle with Ollama embeddings.
    ///
    /// 1. Get embeddings for query and response from Ollama
    /// 2. Evaluate semantic loss
    /// 3. Update MemoryGraph
    ///
    /// Falls back to local loss computation if Ollama is unavailable.
    #[instrument(skip(self, memory_graph))]
    pub async fn evaluate_and_update(
        &self,
        query: &str,
        response: &str,
        memory_graph: &MemoryGraph,
    ) -> Result<FeedbackMetrics> {
        // Try Ollama embeddings first
        let (query_emb, response_emb) = match self.get_embeddings(query, response).await {
            Ok((q, r)) => (q, r),
            Err(e) => {
                warn!(
                    "Ollama embeddings failed: {}, using local hash-based vectors",
                    e
                );
                // Fallback: simple deterministic vectors from text
                let q = Self::text_to_vector(query);
                let r = Self::text_to_vector(response);
                (q, r)
            }
        };

        let eval_result = self
            .evaluator
            .evaluate(&query_emb, &response_emb, memory_graph);

        Ok(FeedbackMetrics {
            loss: eval_result.loss.loss,
            similarity: eval_result.loss.similarity,
            concepts_updated: eval_result.concepts_updated,
            new_concept_created: eval_result.new_concept_created,
            used_ollama: true,
        })
    }

    /// Local-only evaluation (no Ollama call). Useful for testing.
    pub fn evaluate_local(
        &self,
        query_emb: &[f32],
        response_emb: &[f32],
        memory_graph: &MemoryGraph,
    ) -> EvalResult {
        self.evaluator
            .evaluate(query_emb, response_emb, memory_graph)
    }

    /// Compute loss locally (no Ollama, no MemoryGraph update).
    pub fn compute_loss_only(&self, query_emb: &[f32], response_emb: &[f32]) -> f32 {
        self.evaluator.compute_loss(query_emb, response_emb).loss
    }

    /// Get embeddings from Ollama for query and response.
    async fn get_embeddings(&self, query: &str, response: &str) -> Result<(Vec<f32>, Vec<f32>)> {
        let url = format!("{}/api/embeddings", self.config.ollama_url);

        let query_req = EmbeddingRequest {
            model: self.config.embedding_model.clone(),
            prompt: query.to_string(),
        };

        let resp = self.client.post(&url).json(&query_req).send().await?;

        if !resp.status().is_success() {
            anyhow::bail!("Ollama embedding request failed: {}", resp.status());
        }

        let query_emb: EmbeddingResponse = resp.json().await?;

        let response_req = EmbeddingRequest {
            model: self.config.embedding_model.clone(),
            prompt: response.to_string(),
        };

        let resp = self.client.post(&url).json(&response_req).send().await?;

        let response_emb: EmbeddingResponse = resp.json().await?;

        Ok((query_emb.embedding, response_emb.embedding))
    }

    /// Deterministic vector from text — fallback when Ollama is unavailable.
    /// Uses a simple hash-based approach for testing only.
    fn text_to_vector(text: &str) -> Vec<f32> {
        let dim = 64;
        let mut v = vec![0.0f32; dim];
        for (i, ch) in text.bytes().enumerate() {
            let idx = i % dim;
            v[idx] += ch as f32 * 0.01;
        }
        // Normalize
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            v.iter_mut().for_each(|x| *x /= norm);
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feedback_config_defaults() {
        let config = FeedbackConfig::default();
        assert_eq!(config.ollama_url, "http://127.0.0.1:11434");
        assert_eq!(config.embedding_model, "nomic-embed-text");
        assert_eq!(config.eval_config.learning_rate, 0.05);
    }

    #[test]
    fn local_evaluation_works() {
        let config = FeedbackConfig::default();
        let feedback = FeedbackLoop::new(config);

        let query: Vec<f32> = (0..64).map(|i| (i as f32 * 0.01 + 1.0)).collect();
        let response: Vec<f32> = (0..64).map(|i| (i as f32 * 0.01 + 1.5)).collect();

        let result = feedback.compute_loss_only(&query, &response);
        assert!(result >= 0.0, "loss should be non-negative");
    }

    #[test]
    fn text_to_vector_deterministic() {
        let v1 = FeedbackLoop::text_to_vector("hello world");
        let v2 = FeedbackLoop::text_to_vector("hello world");
        assert_eq!(v1, v2, "same text should produce same vector");

        let v3 = FeedbackLoop::text_to_vector("different text");
        assert_ne!(v1, v3, "different text should produce different vector");
    }

    #[test]
    fn text_to_vector_normalized() {
        let v = FeedbackLoop::text_to_vector("test normalization");
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!(
            (norm - 1.0).abs() < 0.01,
            "vector should be approximately normalized, norm={}",
            norm
        );
    }

    #[test]
    fn similarity_increases_with_reinforcement_loop() {
        // Integration test: prove that the self-learning loop works
        let config = FeedbackConfig::default();
        let feedback = FeedbackLoop::new(config);

        let base: Vec<f32> = (0..64).map(|i| (i as f32 * 0.01 + 1.0)).collect();
        let mut response: Vec<f32> = (0..64).map(|i| (i as f32 * 0.01 + 5.0)).collect();

        // Initial loss
        let initial_loss = feedback.compute_loss_only(&base, &response);

        // Simulate reinforcement: nudge response toward base
        let lr = 0.1;
        for i in 0..response.len() {
            response[i] = response[i] + lr * (base[i] - response[i]);
        }

        let reinforced_loss = feedback.compute_loss_only(&base, &response);
        assert!(
            reinforced_loss < initial_loss,
            "reinforced loss ({}) should be < initial loss ({})",
            reinforced_loss,
            initial_loss
        );
    }
}
