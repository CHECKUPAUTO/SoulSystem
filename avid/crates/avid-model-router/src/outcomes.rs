//! Learning from realized routing outcomes (RouteLLM-style).
//!
//! The [`CostAwareRouter`](crate::CostAwareRouter)'s difficulty predictor ships
//! with sensible priors, but it gets better when trained on what *actually*
//! happened: did the cheap model suffice, or was a strong model genuinely
//! needed? An [`OutcomeLog`] accumulates those observations (the gateway logs
//! one per request — e.g. label 1.0 when it had to escalate, 0.0 when the cheap
//! answer was accepted) and turns them into training data the model can learn
//! from. See `docs/RESEARCH_FRONTIER_2026.md` §2.

use serde::{Deserialize, Serialize};

use crate::{Capability, QueryFeatures};

/// One realized routing outcome: the query, the capabilities it needed, and a
/// label for whether a strong model was actually warranted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingOutcome {
    pub query: String,
    pub capabilities: Vec<Capability>,
    /// Label in `[0, 1]`: 1.0 = a strong model was genuinely needed (the cheap
    /// one was escalated / its answer rejected), 0.0 = the cheap model sufficed.
    pub needed_strong: f64,
}

impl RoutingOutcome {
    pub fn new(
        query: impl Into<String>,
        capabilities: Vec<Capability>,
        needed_strong: f64,
    ) -> Self {
        Self {
            query: query.into(),
            capabilities,
            needed_strong: needed_strong.clamp(0.0, 1.0),
        }
    }

    /// The deterministic feature vector for this outcome's query.
    pub fn features(&self) -> QueryFeatures {
        QueryFeatures::extract(&self.query, &self.capabilities)
    }
}

/// An append-only log of routing outcomes, convertible to training data.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OutcomeLog {
    samples: Vec<RoutingOutcome>,
}

impl OutcomeLog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append an outcome.
    pub fn record(&mut self, outcome: RoutingOutcome) {
        self.samples.push(outcome);
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    pub fn samples(&self) -> &[RoutingOutcome] {
        &self.samples
    }

    /// `(features, label)` pairs for [`DifficultyModel::train`](crate::DifficultyModel::train).
    pub fn training_data(&self) -> Vec<(QueryFeatures, f64)> {
        self.samples
            .iter()
            .map(|o| (o.features(), o.needed_strong))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_records_and_exports_training_data() {
        let mut log = OutcomeLog::new();
        assert!(log.is_empty());
        log.record(RoutingOutcome::new("hi", vec![], 0.0));
        log.record(RoutingOutcome::new("deep analysis please", vec![], 1.0));
        assert_eq!(log.len(), 2);

        let data = log.training_data();
        assert_eq!(data.len(), 2);
        assert!(data[0].1.abs() < 1e-9);
        assert!((data[1].1 - 1.0).abs() < 1e-9);
    }

    #[test]
    fn labels_are_clamped() {
        let o = RoutingOutcome::new("q", vec![], 5.0);
        assert!((o.needed_strong - 1.0).abs() < 1e-9);
        let o = RoutingOutcome::new("q", vec![], -2.0);
        assert!(o.needed_strong.abs() < 1e-9);
    }
}
