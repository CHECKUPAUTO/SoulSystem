//! Senate — orchestrates parallel expert calls to Ollama.

use crate::aggregator::{AggregatedResult, AggregationStrategy};
use crate::verify::{VerifiedAggregator, VerifiedResult};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use tracing::{info, instrument};

/// An expert in the senate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Expert {
    /// Model name (Ollama model tag).
    pub model: String,
    /// Role description (e.g., "coding expert", "safety critic").
    pub role: String,
    /// Temperature for generation (0.0 = deterministic, 1.0 = creative).
    pub temperature: f32,
}

impl Expert {
    pub fn new(model: impl Into<String>, role: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            role: role.into(),
            temperature: 0.7,
        }
    }

    pub fn with_temperature(mut self, t: f32) -> Self {
        self.temperature = t;
        self
    }
}

/// A single expert's response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExpertResponse {
    pub model: String,
    pub role: String,
    pub response: String,
    pub duration_ms: u64,
}

/// Ollama chat request.
#[derive(Debug, Serialize)]
struct OllamaRequest {
    model: String,
    prompt: String,
    stream: bool,
    options: OllamaOptions,
}

#[derive(Debug, Serialize)]
struct OllamaOptions {
    temperature: f32,
}

/// Ollama chat response.
#[derive(Debug, Deserialize)]
struct OllamaResponse {
    response: String,
}

/// The Senate — manages parallel expert deliberation.
pub struct Senate {
    experts: Vec<Expert>,
    ollama_url: String,
    client: reqwest::Client,
    critic_model: String,
    strategy: AggregationStrategy,
}

impl Senate {
    pub fn new(ollama_url: impl Into<String>) -> Self {
        Self {
            experts: Vec::new(),
            ollama_url: ollama_url.into(),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .build()
                .expect("failed to build HTTP client"),
            critic_model: "qwen2.5-coder:3b".to_string(),
            strategy: AggregationStrategy::WeightedVote,
        }
    }

    pub fn with_expert(mut self, expert: Expert) -> Self {
        self.experts.push(expert);
        self
    }

    pub fn with_critic(mut self, model: impl Into<String>) -> Self {
        self.critic_model = model.into();
        self
    }

    pub fn with_strategy(mut self, strategy: AggregationStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    /// Deliberate on a prompt using all experts in parallel.
    #[instrument(skip(self, prompt), fields(n_experts = self.experts.len()))]
    pub async fn deliberate(&self, prompt: &str) -> Result<AggregatedResult> {
        info!("Senate deliberation with {} experts", self.experts.len());

        let url = format!("{}/api/generate", self.ollama_url);

        // Spawn all expert calls in parallel
        let futures: Vec<_> = self
            .experts
            .iter()
            .map(|expert| {
                let client = self.client.clone();
                let url = url.clone();
                let model = expert.model.clone();
                let role = expert.role.clone();
                let temperature = expert.temperature;
                let prompt = prompt.to_string();
                let start = std::time::Instant::now();

                async move {
                    let req = OllamaRequest {
                        model: model.clone(),
                        prompt: format!("[Role: {}]\n\n{}", role, prompt),
                        stream: false,
                        options: OllamaOptions { temperature },
                    };

                    let resp = client.post(&url).json(&req).send().await;

                    match resp {
                        Ok(r) if r.status().is_success() => {
                            let body: Result<OllamaResponse, _> = r.json().await;
                            let duration_ms = start.elapsed().as_millis() as u64;
                            body.map(|b| ExpertResponse {
                                model: model.clone(),
                                role,
                                response: b.response,
                                duration_ms,
                            })
                            .map_err(|e| anyhow::anyhow!("Ollama parse error: {}", e))
                        }
                        Ok(r) => Err(anyhow::anyhow!("Ollama HTTP {}", r.status())),
                        Err(e) => Err(anyhow::anyhow!("Ollama request failed: {}", e)),
                    }
                }
            })
            .collect();

        // Await all experts
        let responses: Vec<ExpertResponse> = futures::future::join_all(futures)
            .await
            .into_iter()
            .filter_map(|r| {
                if let Err(e) = &r {
                    info!("Expert failed: {}", e);
                }
                r.ok()
            })
            .collect();

        info!(
            "Senate: {} of {} experts responded",
            responses.len(),
            self.experts.len()
        );

        // Aggregate
        self.strategy.aggregate(&responses, prompt)
    }

    /// Deliberate, then **verify**: run the experts in parallel and pass their
    /// answers through an independent verifier panel, gating the result on
    /// coverage. When no answer clears the panel the result is flagged
    /// `needs_replanning` rather than returning an unverified guess — the
    /// reliability path the plain aggregation strategies don't provide.
    #[instrument(skip(self, prompt, aggregator), fields(n_experts = self.experts.len()))]
    pub async fn deliberate_verified(
        &self,
        prompt: &str,
        aggregator: &VerifiedAggregator,
    ) -> Result<VerifiedResult> {
        let aggregated = self.deliberate(prompt).await?;
        Ok(aggregator.assess(prompt, &aggregated.responses))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn senate_creation() {
        let senate = Senate::new("http://localhost:11434")
            .with_expert(Expert::new("qwen2.5-coder:3b", "general"))
            .with_expert(Expert::new("qwen2.5-coder:3b", "coding"));
        assert_eq!(senate.experts.len(), 2);
    }

    #[test]
    fn expert_with_temperature() {
        let e = Expert::new("test-model", "expert").with_temperature(0.3);
        assert_eq!(e.temperature, 0.3);
    }
}
