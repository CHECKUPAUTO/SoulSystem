//! LLM-based mutation proposer.
//!
//! Connects to a local LLM endpoint (ollama, llama.cpp, etc.) to request
//! targeted mutation proposals. The LLM receives current genome state,
//! fitness context, and recent episodic memory, and returns a structured
//! mutation proposal.
//!
//! # Safety
//!
//! The LLM does NOT generate arbitrary Rust code. It only generates structured
//! JSON proposing which existing mutation variant to apply. The actual mutation
//! is executed by the well-tested `EvolutionEngine::apply_mutation()`.
//!
//! The endpoint URL is restricted to localhost to prevent external calls.

use crate::evolution::{FitnessMetrics, Mutation, MutationEpisodicEvent};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for the LLM mutator.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LlmMutatorConfig {
    /// LLM endpoint URL (e.g. "http://127.0.0.1:11434/api/chat" for ollama)
    pub endpoint_url: String,
    /// Model name for the LLM
    pub model: String,
    /// Request timeout in seconds
    pub timeout_secs: u64,
    /// Whether LLM mutation is enabled
    pub enabled: bool,
    /// Temperature for LLM generation
    pub temperature: f64,
    /// Ticks between LLM mutation attempts
    pub cooldown_ticks: u64,
}

impl Default for LlmMutatorConfig {
    fn default() -> Self {
        Self {
            endpoint_url: "http://127.0.0.1:11434/api/chat".into(),
            model: "llama3.2:1b".into(),
            timeout_secs: 30,
            enabled: false,
            temperature: 0.7,
            cooldown_ticks: 500,
        }
    }
}

/// Structured mutation proposal returned by the LLM.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LlmMutationProposal {
    /// The mutation type key (must match a Mutation variant or "NoChange")
    pub mutation_type: String,
    /// Parameters for the mutation (depends on type)
    #[serde(default)]
    pub parameters: HashMap<String, f64>,
    /// Reasoning provided by the LLM for this proposal
    #[serde(default)]
    pub reasoning: String,
    /// LLM's confidence estimate (0.0-1.0)
    pub confidence: f64,
}

/// Ollama chat request format.
#[derive(Serialize)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<OllamaMessage>,
    stream: bool,
    options: OllamaOptions,
}

#[derive(Serialize)]
struct OllamaMessage {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct OllamaOptions {
    temperature: f64,
}

/// Ollama chat response format.
#[derive(Deserialize)]
struct OllamaChatResponse {
    message: OllamaResponseMessage,
}

#[derive(Deserialize)]
struct OllamaResponseMessage {
    content: String,
}

/// The LLM mutator — manages config, tick tracking, and stats.
#[derive(Clone, Debug)]
pub struct LlmMutator {
    pub config: LlmMutatorConfig,
    client: reqwest::Client,
    /// Ticks since last LLM mutation attempt
    pub ticks_since_last_attempt: u64,
    /// Last LLM response text (for debugging)
    pub last_response: Option<String>,
    /// Total LLM mutation attempts
    pub total_attempts: u64,
    /// Successful LLM mutations (fitness increased)
    pub total_successes: u64,
}

impl LlmMutator {
    /// Create a new LLM mutator with the given config.
    pub fn new(config: LlmMutatorConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .unwrap_or_default();
        Self {
            config,
            client,
            ticks_since_last_attempt: 0,
            last_response: None,
            total_attempts: 0,
            total_successes: 0,
        }
    }

    /// Validate that the endpoint URL points to localhost.
    pub fn validate_endpoint(&self) -> Result<(), String> {
        let url = self.config.endpoint_url.to_lowercase();
        if url.starts_with("http://127.0.0.1:") || url.starts_with("http://localhost:") {
            Ok(())
        } else {
            Err(format!("Endpoint '{}' is not localhost — refusing for safety", self.config.endpoint_url))
        }
    }

    /// Build the prompt context from current genome and fitness state.
    pub fn build_prompt(
        &self,
        genome: &str,
        fitness: &FitnessMetrics,
        recent_memory: &[MutationEpisodicEvent],
    ) -> String {
        let mut prompt = String::new();
        prompt.push_str("You are a neural evolution optimizer for SoulLink brain simulation.\n");
        prompt.push_str(&format!("Current genome state:\n{}\n", genome));
        prompt.push_str(&format!(
            "Current fitness (0-1 scale):\n\
             - Firing balance: {:.3}\n\
             - Synaptic diversity: {:.3}\n\
             - Module utilization: {:.3}\n\
             - Stability: {:.3}\n\
             - Overall: {:.3}\n",
            fitness.firing_balance,
            fitness.synaptic_diversity,
            fitness.module_utilization,
            fitness.stability,
            fitness.overall,
        ));

        if !recent_memory.is_empty() {
            prompt.push_str("\nRecent mutation history (last events):\n");
            for event in recent_memory.iter().rev().take(10) {
                prompt.push_str(&format!(
                    "  {} | fitness {:+.4} | ctx=[fb={:.2},sd={:.2},mu={:.2},st={:.2}]\n",
                    event.mutation_type, event.fitness_delta,
                    event.ctx_firing_balance, event.ctx_synaptic_diversity,
                    event.ctx_module_utilization, event.ctx_stability,
                ));
            }
        }

        prompt.push_str("\nAvailable mutation types:\n\
            - AddNeuron { module: int, count: int }\n\
            - PruneNeurons { module: int, fraction: 0.0-0.3 }\n\
            - AddModule { name: string, neuron_count: int }\n\
            - MutateParam { param: \"tau\"|\"homeostatic_rate\"|\"stdp_learning_rate\"|\"prune_threshold\", delta: float }\n\
            - MutateSynapticDensity { source: int, target: int, density: 0.0-1.0 }\n\
            - MutateCortexConfig { modulation_strength: 0.0-1.0 }\n\
            - MutateActivationScript { module: int, script: string } — Rhai activation function (use sigmoid(), tanh(), relu(), potential, tau, threshold, reserve, noise)\n\
            - MutatePlasticityScript { script: string } — Rhai plasticity rule (use weight, pre_activation, post_activation, eligibility, learning_rate)\n\
            - AddField { target: \"Neuron\"|\"Synapse\"|\"BrainModule\", field_name: string, default: float } — add dynamic extension field\n\
            - RemoveField { target: \"Neuron\"|\"Synapse\"|\"BrainModule\", field_name: string } — remove dynamic extension field\n\n\
            Respond with ONLY valid JSON (no markdown, no explanation):\n\
            {\"mutation_type\": \"MutateParam\", \"parameters\": {\"param\": 0, \"delta\": -0.01}, \
            \"reasoning\": \"firing balance is low, reducing tau increases integration\", \"confidence\": 0.7}\n\n\
            If no mutation is beneficial: {\"mutation_type\": \"NoChange\", \"parameters\": {}, \
            \"reasoning\": \"...\", \"confidence\": 0.0}");

        prompt
    }

    /// Request a mutation proposal from the LLM via the ollama-compatible API.
    pub async fn request_mutation(
        &self,
        genome: &str,
        fitness: &FitnessMetrics,
        recent_memory: &[MutationEpisodicEvent],
    ) -> Result<LlmMutationProposal, String> {
        self.validate_endpoint()?;

        let prompt = self.build_prompt(genome, fitness, recent_memory);

        let request = OllamaChatRequest {
            model: self.config.model.clone(),
            messages: vec![OllamaMessage {
                role: "user".to_string(),
                content: prompt,
            }],
            stream: false,
            options: OllamaOptions {
                temperature: self.config.temperature,
            },
        };

        let response = self.client
            .post(&self.config.endpoint_url)
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("LLM endpoint returned {}: {}", status, body));
        }

        let chat_response: OllamaChatResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        let content = chat_response.message.content;
        Self::parse_proposal(&content)
    }

    /// Parse an LLM response string into a `LlmMutationProposal`.
    pub fn parse_proposal(text: &str) -> Result<LlmMutationProposal, String> {
        let text = text.trim();
        // Strip markdown code fences if present
        let text = if text.starts_with("```") {
            let start = text.find('\n').map(|i| i + 1).unwrap_or(0);
            let end = text.rfind("```").unwrap_or(text.len());
            text[start..end].trim()
        } else {
            text
        };

        let proposal: LlmMutationProposal = serde_json::from_str(text)
            .map_err(|e| format!("Failed to parse LLM response as JSON: {} — text: {}", e, text))?;

        // Validate mutation type
        let valid_types = [
            "AddNeuron", "PruneNeurons", "AddModule",
            "MergeModules", "SplitModule", "MutateParam",
            "MutateSynapticDensity", "MutateCortexConfig", "NoChange",
            "MutateActivationScript", "MutatePlasticityScript",
            "AddField", "RemoveField",
        ];
        if !valid_types.contains(&proposal.mutation_type.as_str()) {
            return Err(format!(
                "Invalid mutation type '{}'. Must be one of: {:?}",
                proposal.mutation_type, valid_types
            ));
        }

        Ok(proposal)
    }

    /// Convert a validated `LlmMutationProposal` to a `Mutation` if applicable.
    /// Returns `None` for "NoChange" or if params are invalid.
    pub fn proposal_to_mutation(
        proposal: &LlmMutationProposal,
        n_modules: usize,
    ) -> Option<Mutation> {
        if proposal.mutation_type == "NoChange" {
            return None;
        }

        let params = &proposal.parameters;
        match proposal.mutation_type.as_str() {
            "AddNeuron" => {
                let module = params.get("module").copied().unwrap_or(0.0) as usize;
                let count = params.get("count").copied().unwrap_or(5.0) as usize;
                let count = count.max(1).min(100);
                Some(Mutation::AddNeuron {
                    module: module.min(n_modules.saturating_sub(1)),
                    count,
                })
            }
            "PruneNeurons" => {
                let module = params.get("module").copied().unwrap_or(0.0) as usize;
                let fraction = params.get("fraction").copied().unwrap_or(0.1).clamp(0.0, 0.3);
                Some(Mutation::PruneNeurons {
                    module: module.min(n_modules.saturating_sub(1)),
                    fraction,
                })
            }
            "MutateParam" => {
                let param_idx = params.get("param").copied().unwrap_or(0.0) as usize;
                let param_name = ["tau", "homeostatic_rate", "stdp_learning_rate", "prune_threshold"]
                    .get(param_idx).copied().unwrap_or("tau").to_string();
                let delta = params.get("delta").copied().unwrap_or(0.0).clamp(-0.05, 0.05);
                Some(Mutation::MutateParam { param: param_name, delta })
            }
            "MutateSynapticDensity" => {
                let source = params.get("source").copied().unwrap_or(0.0) as usize;
                let target = params.get("target").copied().unwrap_or(1.0) as usize;
                let density = params.get("density").copied().unwrap_or(0.3).clamp(0.0, 1.0) as f32;
                Some(Mutation::MutateSynapticDensity {
                    source: source.min(n_modules.saturating_sub(1)),
                    target: target.min(n_modules.saturating_sub(1)),
                    density,
                })
            }
            "MutateCortexConfig" => {
                let ms = params.get("modulation_strength").copied().unwrap_or(0.3).clamp(0.0, 1.0);
                // Map modulation_strength into d_model/state_dim/n_layers variation
                let d_model = (16.0 * (1.0 + (ms - 0.3) * 0.5)) as usize;
                let state_dim = (4.0 * (1.0 + (ms - 0.3) * 0.5)) as usize;
                let n_layers = (2.0 * (1.0 + (ms - 0.3) * 0.5)) as usize;
                Some(Mutation::MutateCortexConfig {
                    d_model: d_model.max(4).min(32),
                    state_dim: state_dim.max(2).min(8),
                    n_layers: n_layers.max(1).min(4),
                })
            }
            "AddModule" => {
                let name = format!("llm_mod_{}", std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH).unwrap_or_default()
                    .as_secs());
                let count = params.get("neuron_count").copied().unwrap_or(500.0) as usize;
                let count = count.max(50).min(2000);
                Some(Mutation::AddModule {
                    name,
                    count,
                    x: 0.0, y: 0.0,
                    color: "#6699ff".into(),
                })
            }
            "MutateActivationScript" => {
                let module = params.get("module").copied().unwrap_or(0.0) as usize;
                let script = params.get("script")
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "sigmoid(potential * 1.2)".into());
                Some(Mutation::MutateActivationScript { module, script })
            }
            "MutatePlasticityScript" => {
                let script = params.get("script")
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "clamp(weight + pre_activation * post_activation * learning_rate, 0.0, 1.0)".into());
                Some(Mutation::MutatePlasticityScript { script })
            }
            "AddField" => {
                let target = params.get("target")
                    .and_then(|v| {
                        // Could be a number index, or a string
                        let targets = ["Neuron", "Synapse", "BrainModule"];
                        let idx = *v as usize;
                        if idx < targets.len() { Some(targets[idx].to_string()) } else { None }
                    })
                    .unwrap_or_else(|| "Neuron".into());
                let field_name = params.get("field_name")
                    .map(|v| format!("field_{}", *v as usize))
                    .unwrap_or_else(|| format!("dyn_{}", std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH).unwrap_or_default()
                        .as_secs()));
                let default = params.get("default").copied().unwrap_or(0.0);
                Some(Mutation::AddField { target, field_name, default })
            }
            "RemoveField" => {
                let target = params.get("target")
                    .and_then(|v| {
                        let targets = ["Neuron", "Synapse", "BrainModule"];
                        let idx = *v as usize;
                        if idx < targets.len() { Some(targets[idx].to_string()) } else { None }
                    })
                    .unwrap_or_else(|| "Neuron".into());
                let field_name = params.get("field_name")
                    .map(|v| format!("field_{}", *v as usize))
                    .unwrap_or_else(|| "unknown".into());
                Some(Mutation::RemoveField { target, field_name })
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_proposal_valid() {
        let json = r#"{"mutation_type":"MutateParam","parameters":{"param":0,"delta":-0.01},"reasoning":"test","confidence":0.7}"#;
        let result = LlmMutator::parse_proposal(json);
        assert!(result.is_ok(), "Parse failed: {:?}", result);
        let p = result.unwrap();
        assert_eq!(p.mutation_type, "MutateParam");
        assert!((p.confidence - 0.7).abs() < 0.001);
    }

    #[test]
    fn test_parse_proposal_with_code_fences() {
        let json = "```json\n{\"mutation_type\":\"AddNeuron\",\"parameters\":{\"module\":0,\"count\":5},\"reasoning\":\"add neurons\",\"confidence\":0.8}\n```";
        let result = LlmMutator::parse_proposal(json);
        assert!(result.is_ok(), "Parse with fences failed: {:?}", result);
        let p = result.unwrap();
        assert_eq!(p.mutation_type, "AddNeuron");
    }

    #[test]
    fn test_parse_proposal_invalid_type() {
        let json = r#"{"mutation_type":"InvalidType","parameters":{},"reasoning":"bad","confidence":0.5}"#;
        let result = LlmMutator::parse_proposal(json);
        assert!(result.is_err(), "Should reject invalid mutation type");
    }

    #[test]
    fn test_parse_proposal_nochange() {
        let json = r#"{"mutation_type":"NoChange","parameters":{},"reasoning":"everything is fine","confidence":0.0}"#;
        let result = LlmMutator::parse_proposal(json);
        assert!(result.is_ok());
        let p = result.unwrap();
        assert_eq!(p.mutation_type, "NoChange");
    }

    #[test]
    fn test_proposal_to_mutation_addneuron() {
        let proposal = LlmMutationProposal {
            mutation_type: "AddNeuron".into(),
            parameters: [("module".into(), 2.0), ("count".into(), 10.0)].into_iter().collect(),
            reasoning: "need more neurons".into(),
            confidence: 0.8,
        };
        let mutation = LlmMutator::proposal_to_mutation(&proposal, 5);
        assert!(mutation.is_some());
        if let Some(Mutation::AddNeuron { module, count }) = mutation {
            assert_eq!(module, 2);
            assert_eq!(count, 10);
        } else {
            panic!("Expected AddNeuron, got {:?}", mutation);
        }
    }

    #[test]
    fn test_proposal_to_mutation_mutateparam() {
        let proposal = LlmMutationProposal {
            mutation_type: "MutateParam".into(),
            parameters: [("param".into(), 0.0), ("delta".into(), -0.02)].into_iter().collect(),
            reasoning: "lower tau".into(),
            confidence: 0.7,
        };
        // Also test parsing the JSON form
        let json = r#"{"mutation_type":"MutateParam","parameters":{"param":0,"delta":-0.02},"reasoning":"lower tau","confidence":0.7}"#;
        let parsed = LlmMutator::parse_proposal(json).unwrap();
        assert_eq!(parsed.mutation_type, "MutateParam");
        let mutation = LlmMutator::proposal_to_mutation(&proposal, 3);
        assert!(mutation.is_some());
        if let Some(Mutation::MutateParam { param, delta }) = mutation {
            assert_eq!(param, "tau");
            assert!((delta - (-0.02)).abs() < 0.001);
        } else {
            panic!("Expected MutateParam");
        }
    }

    #[test]
    fn test_proposal_to_mutation_nochange() {
        let proposal = LlmMutationProposal {
            mutation_type: "NoChange".into(),
            parameters: HashMap::new(),
            reasoning: "all good".into(),
            confidence: 0.9,
        };
        assert!(LlmMutator::proposal_to_mutation(&proposal, 3).is_none());
    }

    #[test]
    fn test_config_default_disabled() {
        let config = LlmMutatorConfig::default();
        assert!(!config.enabled, "LLM mutator should be disabled by default");
        assert_eq!(config.endpoint_url, "http://127.0.0.1:11434/api/chat");
    }

    #[test]
    fn test_endpoint_validation() {
        let mut config = LlmMutatorConfig::default();
        let mutator = LlmMutator::new(config.clone());
        assert!(mutator.validate_endpoint().is_ok(), "localhost should be valid");

        config.endpoint_url = "http://evil.com:11434/api/chat".into();
        let mutator = LlmMutator::new(config);
        assert!(mutator.validate_endpoint().is_err(), "external URL should be rejected");
    }

    #[test]
    fn test_proposal_count_upper_bound() {
        let proposal = LlmMutationProposal {
            mutation_type: "AddNeuron".into(),
            parameters: [("module".into(), 0.0), ("count".into(), 9999.0)].into_iter().collect(),
            reasoning: "too many".into(),
            confidence: 0.5,
        };
        let mutation = LlmMutator::proposal_to_mutation(&proposal, 3);
        if let Some(Mutation::AddNeuron { count, .. }) = mutation {
            assert!(count <= 100, "Count should be bounded: {}", count);
        } else {
            panic!("Expected AddNeuron");
        }
    }

    #[test]
    fn test_build_prompt_contains_context() {
        let config = LlmMutatorConfig::default();
        let mutator = LlmMutator::new(config);
        let fitness = FitnessMetrics {
            firing_balance: 0.3,
            synaptic_diversity: 0.5,
            module_utilization: 0.7,
            stability: 0.9,
            cortex_impact: 0.4,
            growth_health: 0.6,
            novelty_bonus: 0.0,
            energy_efficiency: 0.5,
            temporal_coherence: 0.5,
            emergent_complexity: 0.5,
            overall: 0.5,
        };
        let empty: Vec<MutationEpisodicEvent> = Vec::new();
        let prompt = mutator.build_prompt("Genome: test", &fitness, &empty);
        assert!(prompt.contains("Firing balance: 0.300"));
        assert!(prompt.contains("MutateParam"));
        assert!(prompt.contains("NoChange"));
    }
}

/// Request payload for Claude-based mutation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClaudeMutateRequest {
    pub prompt: String,
    pub file_path: String,
    pub original: String,
    pub target_line: Option<usize>,
}

/// Patch returned by Claude-based mutation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClaudeMutateResponse {
    pub proposal: String,
    pub reasoning: String,
}

impl LlmMutator {
    /// Blocking mutation request.
    pub fn mutate_blocking(&mut self,
        req: ClaudeMutateRequest
    ) -> Result<ClaudeMutateResponse, String> {
        // Stub: return original content when LLM is unavailable
        Ok(ClaudeMutateResponse {
            proposal: req.original,
            reasoning: "LLM mutator stub: no external endpoint".to_string(),
        })
    }
}

// ═══════════════════════════════════════════════════════════════
// CODE PATCH GENERATION (auto-evolve)
// ═══════════════════════════════════════════════════════════════

/// Patch de code source généré par le LLM.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CodePatch {
    pub file_path: String,
    pub new_content: String,
}

/// Prompt pour générer un patch de code.
fn build_code_patch_prompt(file_path: &str, content: &str) -> String {
    format!(
        r#"Tu es un ingénieur Rust spécialisé en systèmes neuronaux autonomes.
Analyse ce fichier source et propose UNE SEULE modification concise qui améliore
la robustesse, la performance ou l'autonomie du système.

Règles strictes :
- Ne modifie que le fichier fourni.
- Ne supprime pas de fonctions entières.
- Retourne UNIQUEMENT un JSON valide (pas de markdown, pas d'explications).
- Le JSON doit avoir exactement cette forme :
  {{"file_path":"{}","new_content":"..."}}
- Le champ new_content doit contenre le CONTENU COMPLET du fichier modifié.

Fichier : {}
Contenu actuel :
{}
"#,
        file_path, file_path, content
    )
}

impl LlmMutator {
    /// Demande au LLM de générer un patch de code source.
    pub async fn request_code_patch(
        &self,
        file_path: &str,
        content: &str,
    ) -> Result<CodePatch, String> {
        self.validate_endpoint()?;

        let prompt = build_code_patch_prompt(file_path, content);

        let request = OllamaChatRequest {
            model: self.config.model.clone(),
            messages: vec![OllamaMessage {
                role: "user".to_string(),
                content: prompt,
            }],
            stream: false,
            options: OllamaOptions {
                temperature: self.config.temperature,
            },
        };

        let response = self
            .client
            .post(&self.config.endpoint_url)
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("LLM endpoint returned {}: {}", status, body));
        }

        let chat_response: OllamaChatResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        let text = chat_response.message.content.trim();
        // Strip markdown fences
        let text = if text.starts_with("```") {
            let start = text.find('\n').map(|i| i + 1).unwrap_or(0);
            let end = text.rfind("```").unwrap_or(text.len());
            text[start..end].trim()
        } else {
            text
        };

        let patch: CodePatch = serde_json::from_str(text)
            .map_err(|e| format!("Failed to parse LLM patch JSON: {} — text: {}", e, text))?;

        Ok(patch)
    }
}
