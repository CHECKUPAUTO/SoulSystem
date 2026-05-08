//! Aggregation strategies for senate deliberation.

use crate::senate::ExpertResponse;
use serde::{Deserialize, Serialize};

/// How to aggregate multiple expert responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AggregationStrategy {
    /// Take the first successful response.
    FirstWins,
    /// Concatenate all responses with headers.
    Concat,
    /// Weighted vote (longer responses get more weight).
    WeightedVote,
}

/// Result of aggregating expert responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedResult {
    /// The final aggregated response.
    pub response: String,
    /// Number of experts that responded.
    pub expert_count: usize,
    /// Strategy used for aggregation.
    pub strategy: String,
    /// Individual expert responses.
    pub responses: Vec<ExpertResponse>,
}

impl AggregationStrategy {
    /// Aggregate expert responses according to this strategy.
    pub fn aggregate(
        &self,
        responses: &[ExpertResponse],
        _original_prompt: &str,
    ) -> anyhow::Result<AggregatedResult> {
        if responses.is_empty() {
            return Ok(AggregatedResult {
                response: String::new(),
                expert_count: 0,
                strategy: format!("{:?}", self),
                responses: Vec::new(),
            });
        }

        let (response, strategy_name) = match self {
            AggregationStrategy::FirstWins => {
                (responses[0].response.clone(), "FirstWins".to_string())
            }
            AggregationStrategy::Concat => {
                let parts: Vec<String> = responses.iter().map(|r| {
                    format!("### {} ({})\n{}", r.role, r.model, r.response)
                }).collect();
                (parts.join("\n\n---\n\n"), "Concat".to_string())
            }
            AggregationStrategy::WeightedVote => {
                // Weight by response length (longer = more thoughtful)
                let total_len: usize = responses.iter().map(|r| r.response.len()).sum();
                if total_len == 0 {
                    (responses[0].response.clone(), "WeightedVote".to_string())
                } else {
                    // Pick the longest response as primary, include others as alternatives
                    let longest = responses.iter()
                        .max_by_key(|r| r.response.len())
                        .unwrap();
                    let mut result = longest.response.clone();
                    if responses.len() > 1 {
                        result.push_str("\n\n---\nAlternatives:\n");
                        for r in responses.iter().filter(|r| r.model != longest.model) {
                            result.push_str(&format!("- {} ({} chars)\n", r.role, r.response.len()));
                        }
                    }
                    (result, "WeightedVote".to_string())
                }
            }
        };

        Ok(AggregatedResult {
            response,
            expert_count: responses.len(),
            strategy: strategy_name,
            responses: responses.to_vec(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_response(model: &str, role: &str, text: &str) -> ExpertResponse {
        ExpertResponse {
            model: model.to_string(),
            role: role.to_string(),
            response: text.to_string(),
            duration_ms: 100,
        }
    }

    #[test]
    fn first_wins_takes_first() {
        let responses = vec![
            fake_response("model-a", "expert", "answer A"),
            fake_response("model-b", "critic", "answer B"),
        ];
        let result = AggregationStrategy::FirstWins.aggregate(&responses, "test").unwrap();
        assert_eq!(result.response, "answer A");
        assert_eq!(result.expert_count, 2);
    }

    #[test]
    fn concat_joins_all() {
        let responses = vec![
            fake_response("model-a", "expert", "A"),
            fake_response("model-b", "critic", "B"),
        ];
        let result = AggregationStrategy::Concat.aggregate(&responses, "test").unwrap();
        assert!(result.response.contains("A"));
        assert!(result.response.contains("B"));
    }

    #[test]
    fn weighted_vote_picks_longest() {
        let responses = vec![
            fake_response("model-a", "expert", "short"),
            fake_response("model-b", "critic", "this is a much longer and more detailed response"),
        ];
        let result = AggregationStrategy::WeightedVote.aggregate(&responses, "test").unwrap();
        assert!(result.response.contains("much longer"));
    }

    #[test]
    fn empty_responses_returns_empty() {
        let result = AggregationStrategy::FirstWins.aggregate(&[], "test").unwrap();
        assert!(result.response.is_empty());
        assert_eq!(result.expert_count, 0);
    }
}