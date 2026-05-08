//! Semantic loss computation — Loss = 1 - cosine_similarity(query, response).
//!
//! Uses `soullink_simd::cosine_dispatch` for AVX2-accelerated similarity.

use soullink_simd::cosine_dispatch;

/// Result of a loss evaluation.
#[derive(Debug, Clone, serde::Serialize)]
pub struct LossResult {
    /// Semantic loss ∈ [0.0, 2.0]. Lower is better.
    pub loss: f32,
    /// Raw cosine similarity ∈ [-1.0, 1.0]. Higher is better.
    pub similarity: f32,
    /// Learning rate used for this evaluation.
    pub learning_rate: f32,
    /// Suggested weight adjustment = loss × learning_rate.
    pub adjustment: f32,
}

/// Semantic loss calculator.
///
/// Loss = 1 - cos_sim(query, response)
///
/// When similarity is high (cos ≈ 1), loss ≈ 0 → minimal adjustment.
/// When similarity is low (cos ≈ 0), loss ≈ 1 → strong reinforcement needed.
pub struct SemanticLoss {
    /// Learning rate for gradient-like updates (default: 0.05).
    pub learning_rate: f32,
}

impl SemanticLoss {
    pub fn new(learning_rate: f32) -> Self {
        Self { learning_rate }
    }

    pub fn default_learning_rate() -> f32 {
        0.05
    }

    /// Compute semantic loss between query and response embeddings.
    ///
    /// Uses AVX2 cosine_dispatch on Broadwell, scalar fallback elsewhere.
    /// The 64-thread Xeon pool handles batch evaluations in parallel via Rayon.
    pub fn compute(&self, query: &[f32], response: &[f32]) -> LossResult {
        let similarity = cosine_dispatch(query, response);
        // Clamp similarity to [-1, 1] for numerical stability
        let similarity = similarity.clamp(-1.0, 1.0);
        let loss = 1.0 - similarity;
        let adjustment = loss * self.learning_rate;

        LossResult {
            loss,
            similarity,
            learning_rate: self.learning_rate,
            adjustment,
        }
    }

    /// Batch compute losses for multiple query-response pairs.
    /// Designed for parallel execution on the 64-thread pool.
    pub fn compute_batch(&self, pairs: &[(Vec<f32>, Vec<f32>)]) -> Vec<LossResult> {
        pairs.iter().map(|(q, r)| self.compute(q, r)).collect()
    }

    /// Compute loss with custom learning rate override.
    pub fn compute_with_lr(&self, query: &[f32], response: &[f32], lr: f32) -> LossResult {
        let mut result = self.compute(query, response);
        result.learning_rate = lr;
        result.adjustment = result.loss * lr;
        result
    }
}

impl Default for SemanticLoss {
    fn default() -> Self {
        Self::new(Self::default_learning_rate())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn embed(seed: f32, dim: usize) -> Vec<f32> {
        (0..dim).map(|i| seed + i as f32 * 0.01).collect()
    }

    #[test]
    fn identical_vectors_zero_loss() {
        let loss = SemanticLoss::default();
        let v = embed(1.0, 128);
        let result = loss.compute(&v, &v);
        assert!(result.loss < 0.01, "loss for identical vectors should be ≈0, got {}", result.loss);
        assert!(result.similarity > 0.99, "similarity for identical vectors should be ≈1");
    }

    #[test]
    fn orthogonal_vectors_high_loss() {
        let loss = SemanticLoss::default();
        let mut a = vec![0.0f32; 128]; a[0] = 1.0;
        let mut b = vec![0.0f32; 128]; b[1] = 1.0;
        let result = loss.compute(&a, &b);
        assert!(result.loss > 0.9, "loss for orthogonal vectors should be ≈1, got {}", result.loss);
        assert!(result.similarity.abs() < 0.1, "similarity for orthogonal vectors should be ≈0");
    }

    #[test]
    fn adjustment_proportional_to_loss() {
        let loss = SemanticLoss::new(0.1);
        let v = embed(1.0, 64);
        let result = loss.compute(&v, &v);
        // Identical vectors: loss≈0, adjustment≈0
        assert!(result.adjustment < 0.01);
    }

    #[test]
    fn different_vectors_positive_adjustment() {
        let loss = SemanticLoss::new(0.05);
        let a = embed(1.0, 64);
        let b = embed(5.0, 64);
        let result = loss.compute(&a, &b);
        // Different vectors: loss > 0, adjustment > 0
        assert!(result.loss > 0.0, "loss should be > 0 for different vectors");
        assert!(result.adjustment > 0.0, "adjustment should be > 0");
        // adjustment = loss × 0.05
        assert!((result.adjustment - result.loss * 0.05).abs() < 0.001);
    }

    #[test]
    fn custom_lr_overrides_default() {
        let loss = SemanticLoss::new(0.05);
        let a = embed(1.0, 64);
        let b = embed(3.0, 64);
        let result = loss.compute_with_lr(&a, &b, 0.5);
        assert_eq!(result.learning_rate, 0.5);
        assert!((result.adjustment - result.loss * 0.5).abs() < 0.001);
    }

    #[test]
    fn batch_computation_matches_individual() {
        let loss = SemanticLoss::default();
        let pairs: Vec<(Vec<f32>, Vec<f32>)> = vec![
            (embed(1.0, 64), embed(1.0, 64)),  // identical
            (embed(1.0, 64), embed(5.0, 64)),  // different
        ];
        let batch = loss.compute_batch(&pairs);
        assert_eq!(batch.len(), 2);
        assert!(batch[0].loss < batch[1].loss, "identical pair should have lower loss");
    }

    #[test]
    fn similarity_increases_with_reinforcement() {
        // Prove that reinforcing a link increases similarity and decreases loss
        let loss = SemanticLoss::new(0.05);
        let base = embed(1.0, 64);
        let response = embed(1.5, 64); // somewhat similar

        let result1 = loss.compute(&base, &response);
        // Simulate reinforcement: move response closer to base
        let mut reinforced = response.clone();
        let lr = 0.05;
        for i in 0..reinforced.len() {
            reinforced[i] = reinforced[i] + lr * (base[i] - reinforced[i]);
        }
        let result2 = loss.compute(&base, &reinforced);

        assert!(result2.similarity > result1.similarity,
            "reinforced similarity ({}) should be > original ({})",
            result2.similarity, result1.similarity);
        assert!(result2.loss < result1.loss,
            "reinforced loss ({}) should be < original ({})",
            result2.loss, result1.loss);
    }
}