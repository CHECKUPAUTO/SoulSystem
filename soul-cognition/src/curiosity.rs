//! Artificial curiosity — the one genuinely missing module.
//!
//! When no external goal is pending, the agent should still have something
//! useful to do: explore where its world-model is weakest. This module scores
//! candidate *probes* by novelty (how unlike anything already in semantic
//! memory they are) and emits the most informative ones as low-stakes,
//! read-only (Level-0) exploration suggestions.
//!
//! It only ever *suggests* — it never executes. Suggestions flow back into the
//! planner as ordinary goals, which are then subject to the permission gate
//! like anything else.

use serde::{Deserialize, Serialize};

/// Cosine similarity between two equal-length vectors. Returns 0.0 for
/// mismatched lengths or zero vectors (treated as maximally novel).
fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    let denom = (na.sqrt() * nb.sqrt()).max(1e-12);
    dot / denom
}

/// A candidate thing the agent could investigate, with its embedding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Probe {
    /// Human-readable description of what to explore.
    pub topic: String,
    /// Embedding of `topic` in the same space as semantic memory.
    pub embedding: Vec<f32>,
}

impl Probe {
    pub fn new(topic: impl Into<String>, embedding: Vec<f32>) -> Self {
        Self {
            topic: topic.into(),
            embedding,
        }
    }
}

/// A probe ranked by its intrinsic interestingness.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScoredProbe {
    pub topic: String,
    /// Novelty in `[0, 1]`: `1 - max cosine similarity` to known memory.
    pub novelty: f32,
}

/// Scores candidate probes against the embeddings the agent already knows.
#[derive(Debug, Default, Clone)]
pub struct Curiosity {
    /// Embeddings of facts already consolidated into semantic memory.
    known: Vec<Vec<f32>>,
}

impl Curiosity {
    pub fn new() -> Self {
        Self::default()
    }

    /// Seed with the embeddings currently held in semantic memory.
    pub fn with_known(known: Vec<Vec<f32>>) -> Self {
        Self { known }
    }

    /// Record that something is now known (e.g. after a probe is explored and
    /// consolidated), so it stops looking novel.
    pub fn learn(&mut self, embedding: Vec<f32>) {
        self.known.push(embedding);
    }

    /// Novelty of a single embedding: `1 - max similarity` to anything known.
    /// With no prior knowledge everything is maximally novel (`1.0`).
    pub fn novelty(&self, embedding: &[f32]) -> f32 {
        let max_sim = self
            .known
            .iter()
            .map(|k| cosine(k, embedding))
            .fold(0.0f32, f32::max);
        (1.0 - max_sim).clamp(0.0, 1.0)
    }

    /// Rank probes most-novel first. Probes below `min_novelty` are dropped so
    /// the agent does not waste cycles re-exploring well-understood ground.
    pub fn rank(&self, probes: &[Probe], min_novelty: f32) -> Vec<ScoredProbe> {
        let mut scored: Vec<ScoredProbe> = probes
            .iter()
            .map(|p| ScoredProbe {
                topic: p.topic.clone(),
                novelty: self.novelty(&p.embedding),
            })
            .filter(|s| s.novelty >= min_novelty)
            .collect();
        scored.sort_by(|a, b| {
            b.novelty
                .partial_cmp(&a.novelty)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored
    }

    /// The single most interesting probe to pursue next, if any clears the bar.
    pub fn most_interesting(&self, probes: &[Probe], min_novelty: f32) -> Option<ScoredProbe> {
        self.rank(probes, min_novelty).into_iter().next()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn everything_is_novel_with_no_knowledge() {
        let c = Curiosity::new();
        assert_eq!(c.novelty(&[1.0, 0.0, 0.0]), 1.0);
    }

    #[test]
    fn known_thing_has_low_novelty() {
        let c = Curiosity::with_known(vec![vec![1.0, 0.0, 0.0]]);
        // Identical vector → similarity 1 → novelty 0.
        assert!(c.novelty(&[1.0, 0.0, 0.0]) < 1e-3);
        // Orthogonal vector → similarity 0 → novelty 1.
        assert!(c.novelty(&[0.0, 1.0, 0.0]) > 0.99);
    }

    #[test]
    fn ranking_orders_by_novelty_and_filters() {
        let c = Curiosity::with_known(vec![vec![1.0, 0.0, 0.0]]);
        let probes = vec![
            Probe::new("seen", vec![1.0, 0.0, 0.0]),       // novelty ~0
            Probe::new("orthogonal", vec![0.0, 1.0, 0.0]), // novelty ~1
            Probe::new("partial", vec![1.0, 1.0, 0.0]),    // novelty ~0.29
        ];
        let ranked = c.rank(&probes, 0.1);
        // "seen" is filtered out; "orthogonal" ranks first.
        assert_eq!(ranked.len(), 2);
        assert_eq!(ranked[0].topic, "orthogonal");
        assert_eq!(ranked[1].topic, "partial");
    }

    #[test]
    fn learning_reduces_future_novelty() {
        let mut c = Curiosity::new();
        let v = vec![0.5, 0.5, 0.0];
        assert_eq!(c.novelty(&v), 1.0);
        c.learn(v.clone());
        assert!(c.novelty(&v) < 1e-3);
    }

    #[test]
    fn most_interesting_picks_the_top() {
        let c = Curiosity::with_known(vec![vec![1.0, 0.0]]);
        let probes = vec![
            Probe::new("a", vec![1.0, 0.0]),
            Probe::new("b", vec![0.0, 1.0]),
        ];
        assert_eq!(c.most_interesting(&probes, 0.1).unwrap().topic, "b");
    }

    #[test]
    fn mismatched_length_is_maximally_novel() {
        let c = Curiosity::with_known(vec![vec![1.0, 0.0, 0.0]]);
        assert_eq!(c.novelty(&[1.0, 0.0]), 1.0);
    }
}
