//! Intelligent Multi-LLM Router
//!
//! Routes tasks to the optimal LLM model based on:
//! - Task domain (code, system, research, creative)
//! - Task complexity score
//! - Model cost and latency profile
//! - Recent success/failure rates per model
//!
//! ## Routing tiers:
//! - **Fast local**: simple tasks → qwen3:4b or equivalent
//! - **Code specialist**: code tasks → codellama or deepseek-coder
//! - **General purpose**: medium tasks → default model
//! - **Cloud premium**: complex tasks → deepseek-v4 or cloud equivalent
//!
//! ## Fallback chain:
//! Each model tier has a fallback provider. On failure, the router
//! tries the next model in the tier's fallback chain.

use crate::strategy::TaskDomain;
use serde::{Deserialize, Serialize};
use soul_llm::{ChatMessage, ChatResponse, LlmConfig, OllamaClient, ProviderKind, ToolSchema};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Tier classification for LLM models.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelTier {
    /// Local fast model (e.g., qwen3:4b) for trivial tasks.
    FastLocal,
    /// Code-specialized model for development tasks.
    CodeSpecialist,
    /// General-purpose medium model.
    GeneralPurpose,
    /// Cloud premium model for complex/creative tasks.
    CloudPremium,
}

impl std::fmt::Display for ModelTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FastLocal => write!(f, "FastLocal"),
            Self::CodeSpecialist => write!(f, "CodeSpecialist"),
            Self::GeneralPurpose => write!(f, "GeneralPurpose"),
            Self::CloudPremium => write!(f, "CloudPremium"),
        }
    }
}

/// Configuration for a routed model entry.
#[derive(Debug, Clone)]
pub struct RouterEntryConfig {
    /// Display name for this model entry.
    pub name: String,
    /// LLM config (provider, model, url, etc.).
    pub llm_config: LlmConfig,
    /// Routing tier.
    pub tier: ModelTier,
    /// Domains this model specializes in.
    pub domains: Vec<TaskDomain>,
    /// Minimum complexity (0.0-1.0) to use this model.
    pub min_complexity: f32,
    /// Maximum complexity to use this model (1.0 = no max).
    pub max_complexity: f32,
    /// Names of fallback entries (in order) to try on failure.
    pub fallback_names: Vec<String>,
}

/// Overall router configuration.
#[derive(Debug, Clone)]
pub struct RouterConfig {
    /// Registered model entries.
    pub entries: Vec<RouterEntryConfig>,
    /// Maximum consecutive failures before disabling an entry.
    pub max_consecutive_failures: usize,
    /// Cooldown duration in seconds before retrying a failed entry.
    pub cooldown_secs: u64,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            entries: vec![
                // Tier 1: Fast local for trivial tasks and system ops
                RouterEntryConfig {
                    name: "qwen3-4b".into(),
                    llm_config: LlmConfig {
                        provider: ProviderKind::Ollama,
                        base_url: "http://127.0.0.1:11434".into(),
                        model: "qwen3:4b".into(),
                        temperature: 0.3,
                        max_tokens: 2048,
                        http_timeout: std::time::Duration::from_secs(60),
                        connect_timeout: std::time::Duration::from_secs(5),
                        auth_token: None,
                        goal_token_budget: 500000,
                        tokens_per_minute_budget: 50000,
                        pool_max_idle: 2,
                        pool_idle_timeout: std::time::Duration::from_secs(30),
                    },
                    tier: ModelTier::FastLocal,
                    domains: vec![TaskDomain::System, TaskDomain::General],
                    min_complexity: 0.0,
                    max_complexity: 0.4,
                    fallback_names: vec!["default".into()],
                },
                // Tier 2: General-purpose default model
                RouterEntryConfig {
                    name: "default".into(),
                    llm_config: LlmConfig {
                        provider: ProviderKind::Ollama,
                        base_url: "http://127.0.0.1:11434".into(),
                        model: "qwen3:8b".into(),
                        temperature: 0.5,
                        max_tokens: 4096,
                        http_timeout: std::time::Duration::from_secs(120),
                        connect_timeout: std::time::Duration::from_secs(5),
                        auth_token: None,
                        goal_token_budget: 1000000,
                        tokens_per_minute_budget: 100000,
                        pool_max_idle: 2,
                        pool_idle_timeout: std::time::Duration::from_secs(30),
                    },
                    tier: ModelTier::GeneralPurpose,
                    domains: vec![
                        TaskDomain::General,
                        TaskDomain::System,
                        TaskDomain::Research,
                        TaskDomain::Creative,
                        TaskDomain::Code,
                    ],
                    min_complexity: 0.0,
                    max_complexity: 1.0,
                    fallback_names: vec!["cloud".into()],
                },
                // Tier 3: Cloud premium for complex/creative tasks
                RouterEntryConfig {
                    name: "cloud".into(),
                    llm_config: LlmConfig {
                        provider: ProviderKind::Ollama,
                        base_url: "http://127.0.0.1:11434".into(),
                        model: "deepseek-v4:cloud".into(),
                        temperature: 0.7,
                        max_tokens: 8192,
                        http_timeout: std::time::Duration::from_secs(180),
                        connect_timeout: std::time::Duration::from_secs(10),
                        auth_token: None,
                        goal_token_budget: 2000000,
                        tokens_per_minute_budget: 200000,
                        pool_max_idle: 2,
                        pool_idle_timeout: std::time::Duration::from_secs(30),
                    },
                    tier: ModelTier::CloudPremium,
                    domains: vec![TaskDomain::Creative, TaskDomain::Research],
                    min_complexity: 0.5,
                    max_complexity: 1.0,
                    fallback_names: vec!["default".into()],
                },
            ],
            max_consecutive_failures: 3,
            cooldown_secs: 60,
        }
    }
}

/// Statistics tracked per routed model.
#[derive(Debug, Clone, Default)]
pub struct ModelStats {
    pub total_calls: usize,
    pub successful_calls: usize,
    pub failed_calls: usize,
    pub consecutive_failures: usize,
    pub total_latency_ms: u64,
    pub last_failure_time: Option<std::time::Instant>,
}

/// The multi-provider router wrapping multiple LLM clients.
pub struct LlmRouter {
    /// Named clients indexed by entry name.
    clients: HashMap<String, OllamaClient>,
    /// Configuration entries keyed by name.
    entries: Vec<RouterEntryConfig>,
    /// Performance statistics per entry.
    stats: Arc<RwLock<HashMap<String, ModelStats>>>,
    /// Maximum consecutive failures before cooldown.
    max_consecutive_failures: usize,
    /// Cooldown duration.
    cooldown_secs: u64,
}

impl LlmRouter {
    /// Build a router from a configuration.
    pub fn new(config: RouterConfig) -> Self {
        let mut clients = HashMap::new();
        for entry in &config.entries {
            let client = OllamaClient::new(entry.llm_config.clone());
            clients.insert(entry.name.clone(), client);
        }
        let mut stats = HashMap::new();
        for entry in &config.entries {
            stats.insert(entry.name.clone(), ModelStats::default());
        }
        Self {
            clients,
            entries: config.entries,
            stats: Arc::new(RwLock::new(stats)),
            max_consecutive_failures: config.max_consecutive_failures,
            cooldown_secs: config.cooldown_secs,
        }
    }

    /// Build a router with a single model (degenerate case, no routing).
    pub fn single(model_name: &str, config: LlmConfig) -> Self {
        let router_config = RouterConfig {
            entries: vec![RouterEntryConfig {
                name: model_name.to_string(),
                llm_config: config,
                tier: ModelTier::GeneralPurpose,
                domains: vec![
                    TaskDomain::General,
                    TaskDomain::System,
                    TaskDomain::Code,
                    TaskDomain::Research,
                    TaskDomain::Creative,
                ],
                min_complexity: 0.0,
                max_complexity: 1.0,
                fallback_names: vec![],
            }],
            max_consecutive_failures: 3,
            cooldown_secs: 60,
        };
        Self::new(router_config)
    }

    /// Select the best model for a task based on domain and complexity.
    pub fn select_best(&self, task: &str, domain: TaskDomain, complexity: f32) -> &str {
        // Score each entry for this task
        let mut best_name = &self.entries[0].name;
        let mut best_score: f32 = f32::NEG_INFINITY;

        for entry in &self.entries {
            // Check if model is in cooldown
            let stats_lock = self.stats.try_read();
            if let Ok(stats) = stats_lock {
                if let Some(s) = stats.get(&entry.name) {
                    if s.consecutive_failures >= self.max_consecutive_failures {
                        if let Some(last_fail) = s.last_failure_time {
                            if last_fail.elapsed().as_secs() < self.cooldown_secs {
                                tracing::debug!(
                                    "Router: skipping {} (in cooldown, {} failures)",
                                    entry.name,
                                    s.consecutive_failures
                                );
                                continue;
                            }
                        }
                    }
                }
            }

            // Score: domain match + complexity fit + success rate
            let domain_score: f32 = if entry.domains.contains(&domain) { 1.0 } else { 0.2 };
            let complexity_fit: f32 = if complexity >= entry.min_complexity
                && complexity <= entry.max_complexity
            {
                1.0
            } else {
                let dist_from_range = if complexity < entry.min_complexity {
                    entry.min_complexity - complexity
                } else {
                    complexity - entry.max_complexity
                };
                (1.0 - dist_from_range).max(0.0)
            };

            let success_bonus = if let Ok(stats) = self.stats.try_read() {
                if let Some(s) = stats.get(&entry.name) {
                    if s.total_calls > 0 {
                        s.successful_calls as f64 / s.total_calls as f64
                    } else {
                        0.5 // neutral for untried models
                    }
                } else {
                    0.5
                }
            } else {
                0.5
            };

            // Cloud models have a cost penalty (prefer cheaper for simple tasks)
            let cost_penalty = match entry.tier {
                ModelTier::CloudPremium if complexity < 0.5 => -0.3,
                ModelTier::FastLocal if complexity > 0.6 => -0.2,
                ModelTier::CodeSpecialist if domain != TaskDomain::Code => -0.1,
                _ => 0.0,
            };

            let score = domain_score * 0.3_f32 + complexity_fit * 0.35_f32 + success_bonus as f32 * 0.25_f32 + cost_penalty as f32;

            if score > best_score {
                best_score = score;
                best_name = &entry.name;
            }
        }

        best_name
    }

    /// Get fallback names for a given entry name.
    fn fallback_chain(&self, entry_name: &str) -> Vec<&str> {
        for entry in &self.entries {
            if entry.name == entry_name {
                return entry.fallback_names.iter().map(|s| s.as_str()).collect();
            }
        }
        vec![]
    }

    /// Execute a chat with routing and fallback.
    pub async fn chat(
        &self,
        task_context: &str,
        domain: TaskDomain,
        complexity: f32,
        messages: &[ChatMessage],
        tools: Option<&[ToolSchema]>,
    ) -> Result<ChatResponse, String> {
        let primary = self.select_best(task_context, domain, complexity).to_string();
        let mut tried = vec![primary.clone()];

        // First attempt with best model
        match self.try_chat(&primary, messages, tools).await {
            Ok(resp) => {
                self.record_success(&primary, 0).await;
                return Ok(resp);
            }
            Err(e) => {
                tracing::warn!("Router: primary model {} failed: {}", primary, e);
                self.record_failure(&primary).await;
            }
        }

        // Fallback chain
        let fallbacks = self.fallback_chain(&primary);
        for fallback_name in &fallbacks {
            if tried.contains(&fallback_name.to_string()) {
                continue;
            }
            tried.push(fallback_name.to_string());

            match self.try_chat(fallback_name, messages, tools).await {
                Ok(resp) => {
                    self.record_success(fallback_name, 0).await;
                    return Ok(resp);
                }
                Err(e) => {
                    tracing::warn!("Router: fallback {} failed: {}", fallback_name, e);
                    self.record_failure(fallback_name).await;
                }
            }
        }

        // Last resort: try ALL remaining models
        for entry in &self.entries {
            if tried.contains(&entry.name) {
                continue;
            }
            match self.try_chat(&entry.name, messages, tools).await {
                Ok(resp) => {
                    self.record_success(&entry.name, 0).await;
                    return Ok(resp);
                }
                Err(e) => {
                    tracing::warn!("Router: last-resort {} failed: {}", entry.name, e);
                    self.record_failure(&entry.name).await;
                }
            }
        }

        Err(format!(
            "All {} models failed. Tried: {}",
            self.entries.len(),
            tried.join(" → ")
        ))
    }

    /// Execute a generate call with routing.
    pub async fn generate(
        &self,
        task_context: &str,
        domain: TaskDomain,
        complexity: f32,
        prompt: &str,
    ) -> Result<ChatResponse, String> {
        let primary = self.select_best(task_context, domain, complexity).to_string();
        let mut tried = vec![primary.clone()];

        match self.try_generate(&primary, prompt).await {
            Ok(resp) => {
                self.record_success(&primary, 0).await;
                return Ok(resp);
            }
            Err(e) => {
                tracing::warn!("Router generate: {} failed: {}", primary, e);
                self.record_failure(&primary).await;
            }
        }

        for fallback_name in &self.fallback_chain(&primary) {
            if tried.contains(&fallback_name.to_string()) {
                continue;
            }
            tried.push(fallback_name.to_string());

            match self.try_generate(fallback_name, prompt).await {
                Ok(resp) => {
                    self.record_success(fallback_name, 0).await;
                    return Ok(resp);
                }
                Err(e) => {
                    tracing::warn!("Router generate fallback: {} failed: {}", fallback_name, e);
                    self.record_failure(fallback_name).await;
                }
            }
        }

        // Last resort: try remaining models
        for entry in &self.entries {
            if tried.contains(&entry.name) {
                continue;
            }
            match self.try_generate(&entry.name, prompt).await {
                Ok(resp) => {
                    self.record_success(&entry.name, 0).await;
                    return Ok(resp);
                }
                Err(_) => {
                    self.record_failure(&entry.name).await;
                }
            }
        }

        Err("All models exhausted".to_string())
    }

    async fn try_chat(
        &self,
        name: &str,
        messages: &[ChatMessage],
        tools: Option<&[ToolSchema]>,
    ) -> Result<ChatResponse, String> {
        let client = self
            .clients
            .get(name)
            .ok_or_else(|| format!("Unknown model: {}", name))?;
        client.chat(messages, tools).await
    }

    async fn try_generate(
        &self,
        name: &str,
        prompt: &str,
    ) -> Result<ChatResponse, String> {
        let client = self
            .clients
            .get(name)
            .ok_or_else(|| format!("Unknown model: {}", name))?;
        client.generate(prompt).await
    }

    async fn record_success(&self, name: &str, _latency_ms: u64) {
        let mut stats = self.stats.write().await;
        if let Some(s) = stats.get_mut(name) {
            s.total_calls += 1;
            s.successful_calls += 1;
            s.consecutive_failures = 0;
        }
    }

    async fn record_failure(&self, name: &str) {
        let mut stats = self.stats.write().await;
        if let Some(s) = stats.get_mut(name) {
            s.total_calls += 1;
            s.failed_calls += 1;
            s.consecutive_failures += 1;
            s.last_failure_time = Some(std::time::Instant::now());
        }
    }

    /// Get current router statistics.
    pub async fn stats(&self) -> HashMap<String, ModelStats> {
        self.stats.read().await.clone()
    }

    /// Reset failure counters (e.g., after a model comes back online).
    pub async fn reset_failures(&self, name: &str) {
        let mut stats = self.stats.write().await;
        if let Some(s) = stats.get_mut(name) {
            s.consecutive_failures = 0;
            s.last_failure_time = None;
        }
    }

    /// Add or replace a model entry at runtime.
    pub fn add_model(&mut self, entry: RouterEntryConfig) {
        let name = entry.name.clone();
        let client = OllamaClient::new(entry.llm_config.clone());
        self.clients.insert(name.clone(), client);
        // Update or add entry
        if let Some(existing) = self.entries.iter_mut().find(|e| e.name == name) {
            *existing = entry;
        } else {
            self.entries.push(entry);
        }
    }

    /// Number of available (non-cooldown) models.
    pub fn available_count(&self) -> usize {
        let stats = self.stats.try_read();
        self.entries
            .iter()
            .filter(|e| {
                if let Ok(ref s) = stats {
                    if let Some(stat) = s.get(&e.name) {
                        if stat.consecutive_failures >= self.max_consecutive_failures {
                            if let Some(t) = stat.last_failure_time {
                                return t.elapsed().as_secs() >= self.cooldown_secs;
                            }
                        }
                    }
                }
                true
            })
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_config(model: &str) -> LlmConfig {
        LlmConfig {
            provider: ProviderKind::Ollama,
            base_url: "http://localhost:11434".into(),
            model: model.into(),
            temperature: 0.5,
            max_tokens: 1024,
            http_timeout: std::time::Duration::from_secs(5),
            connect_timeout: std::time::Duration::from_secs(1),
            auth_token: None,
            goal_token_budget: 1000,
            tokens_per_minute_budget: 10000,
            pool_max_idle: 1,
            pool_idle_timeout: std::time::Duration::from_secs(30),
        }
    }

    fn make_mini_router() -> LlmRouter {
        LlmRouter::new(RouterConfig {
            entries: vec![
                RouterEntryConfig {
                    name: "fast".into(),
                    llm_config: make_test_config("qwen3:4b"),
                    tier: ModelTier::FastLocal,
                    domains: vec![TaskDomain::General, TaskDomain::System],
                    min_complexity: 0.0,
                    max_complexity: 0.4,
                    fallback_names: vec!["general".into()],
                },
                RouterEntryConfig {
                    name: "general".into(),
                    llm_config: make_test_config("qwen3:8b"),
                    tier: ModelTier::GeneralPurpose,
                    domains: vec![TaskDomain::General],
                    min_complexity: 0.0,
                    max_complexity: 1.0,
                    fallback_names: vec!["cloud".into()],
                },
                RouterEntryConfig {
                    name: "cloud".into(),
                    llm_config: make_test_config("deepseek-v4"),
                    tier: ModelTier::CloudPremium,
                    domains: vec![TaskDomain::Creative, TaskDomain::Research],
                    min_complexity: 0.5,
                    max_complexity: 1.0,
                    fallback_names: vec!["general".into()],
                },
            ],
            max_consecutive_failures: 3,
            cooldown_secs: 60,
        })
    }

    #[test]
    fn selects_fast_for_simple_task() {
        let router = make_mini_router();
        let name = router.select_best("hello", TaskDomain::General, 0.1);
        assert_eq!(name, "fast");
    }

    #[test]
    fn selects_general_for_medium_task() {
        let router = make_mini_router();
        let name = router.select_best(
            "Explain how the Rust borrow checker works in detail",
            TaskDomain::Code,
            0.5,
        );
        assert_eq!(name, "general");
    }

    #[test]
    fn selects_cloud_for_complex_creative() {
        let router = make_mini_router();
        let name = router.select_best(
            "Design a novel distributed system architecture",
            TaskDomain::Creative,
            0.9,
        );
        assert_eq!(name, "cloud");
    }

    #[test]
    fn selects_system_for_system_tasks() {
        let router = make_mini_router();
        let name = router.select_best("Check disk usage", TaskDomain::System, 0.2);
        // fast is the only one with System domain and fits complexity
        assert_eq!(name, "fast");
    }

    #[test]
    fn fallback_chain_basic() {
        let router = make_mini_router();
        let chain = router.fallback_chain("fast");
        assert_eq!(chain, vec!["general"]);
    }

    #[test]
    fn router_config_default() {
        let cfg = RouterConfig::default();
        assert_eq!(cfg.entries.len(), 3);
        assert_eq!(cfg.max_consecutive_failures, 3);
        assert_eq!(cfg.cooldown_secs, 60);
        assert_eq!(cfg.entries[0].name, "qwen3-4b");
        assert_eq!(cfg.entries[1].name, "default");
        assert_eq!(cfg.entries[2].name, "cloud");
    }
}
