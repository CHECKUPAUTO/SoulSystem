use serde::{Deserialize, Serialize};
use thiserror::Error;

// ─── Errors ──────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum InferenceError {
    #[error("Invalid configuration: {0}")]
    Config(String),
}

pub type Result<T> = std::result::Result<T, InferenceError>;

// ─── Three-Axis Control ─────────────────────────────────────────────────────

/// Axis 1: Capability tier — which models/tools to use
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum CapabilityTier {
    /// Local small models only (qwen3:4b, fast inference)
    Local,
    /// Fast cloud/local models (qwen3:8b, good balance)
    Fast,
    /// Balanced: mix of local + cloud
    Balanced,
    /// Powerful: large models, full context
    Powerful,
}

impl CapabilityTier {
    pub fn models(&self) -> Vec<&str> {
        match self {
            Self::Local => vec!["qwen3:4b", "phi3", "gemma2:2b"],
            Self::Fast => vec!["qwen3:8b", "deepseek-r1:7b", "gemma3:4b"],
            Self::Balanced => vec!["qwen3:8b", "deepseek-r1:7b", "nomic-embed-text"],
            Self::Powerful => vec!["qwen3:14b", "deepseek-r1:14b", "gemma3:27b"],
        }
    }

    pub fn max_tokens(&self) -> usize {
        match self {
            Self::Local => 2048,
            Self::Fast => 4096,
            Self::Balanced => 8192,
            Self::Powerful => 32768,
        }
    }

    pub fn cost_per_1k_tokens(&self) -> f64 {
        match self {
            Self::Local => 0.0,
            Self::Fast => 0.001,
            Self::Balanced => 0.005,
            Self::Powerful => 0.02,
        }
    }
}

/// Axis 2: Thinking level — how much reasoning to apply
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ThinkingLevel {
    /// No reasoning, direct answer
    Off,
    /// Minimal reasoning, quick check
    Minimal,
    /// Low reasoning, single-pass
    Low,
    /// Medium reasoning, some chain-of-thought
    Medium,
    /// High reasoning, deep chain-of-thought
    High,
}

impl ThinkingLevel {
    pub fn prompt_prefix(&self) -> &str {
        match self {
            Self::Off => "",
            Self::Minimal => "Think briefly: ",
            Self::Low => "Think step by step briefly: ",
            Self::Medium => "Think through this carefully:\n",
            Self::High => "Let me think through this very carefully, considering all angles:\n",
        }
    }

    pub fn max_reasoning_tokens(&self) -> usize {
        match self {
            Self::Off => 0,
            Self::Minimal => 256,
            Self::Low => 512,
            Self::Medium => 2048,
            Self::High => 8192,
        }
    }
}

/// Axis 3: Context class — how much context to include
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ContextClass {
    /// Minimal: just the immediate task
    Small,
    /// Recent context only
    Medium,
    /// Full conversation + memory
    Large,
    /// Everything available
    Full,
}

impl ContextClass {
    pub fn max_messages(&self) -> usize {
        match self {
            Self::Small => 2,
            Self::Medium => 10,
            Self::Large => 50,
            Self::Full => usize::MAX,
        }
    }

    pub fn include_memory(&self) -> bool {
        matches!(self, Self::Large | Self::Full)
    }

    pub fn include_history(&self) -> bool {
        !matches!(self, Self::Small)
    }
}

// ─── Inference Profile ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceProfile {
    pub capability: CapabilityTier,
    pub thinking: ThinkingLevel,
    pub context: ContextClass,
}

impl InferenceProfile {
    pub fn new(
        capability: CapabilityTier,
        thinking: ThinkingLevel,
        context: ContextClass,
    ) -> Self {
        Self {
            capability,
            thinking,
            context,
        }
    }

    pub fn estimated_cost_per_query(&self) -> f64 {
        let base_cost = self.capability.cost_per_1k_tokens();
        let token_multiplier = match self.context {
            ContextClass::Small => 0.5,
            ContextClass::Medium => 2.0,
            ContextClass::Large => 5.0,
            ContextClass::Full => 10.0,
        };
        base_cost * token_multiplier
    }

    pub fn estimated_latency_ms(&self) -> u64 {
        let base_latency = match self.capability {
            CapabilityTier::Local => 100,
            CapabilityTier::Fast => 500,
            CapabilityTier::Balanced => 1000,
            CapabilityTier::Powerful => 3000,
        };
        let thinking_multiplier = match self.thinking {
            ThinkingLevel::Off => 1.0,
            ThinkingLevel::Minimal => 1.2,
            ThinkingLevel::Low => 1.5,
            ThinkingLevel::Medium => 2.0,
            ThinkingLevel::High => 3.0,
        };
        (base_latency as f64 * thinking_multiplier) as u64
    }
}

// ─── Task Complexity Detector ────────────────────────────────────────────────

pub struct InferenceController;

impl InferenceController {
    /// Auto-select profile based on task description
    pub fn select_profile(task_description: &str) -> InferenceProfile {
        let complexity = Self::estimate_complexity(task_description);

        let capability = if complexity < 3 {
            CapabilityTier::Local
        } else if complexity < 5 {
            CapabilityTier::Fast
        } else if complexity < 7 {
            CapabilityTier::Balanced
        } else {
            CapabilityTier::Powerful
        };

        let thinking = if complexity < 2 {
            ThinkingLevel::Off
        } else if complexity < 4 {
            ThinkingLevel::Minimal
        } else if complexity < 6 {
            ThinkingLevel::Low
        } else if complexity < 8 {
            ThinkingLevel::Medium
        } else {
            ThinkingLevel::High
        };

        let context = if task_description.len() < 50 {
            ContextClass::Small
        } else if task_description.len() < 200 {
            ContextClass::Medium
        } else if task_description.len() < 1000 {
            ContextClass::Large
        } else {
            ContextClass::Full
        };

        InferenceProfile::new(capability, thinking, context)
    }

    pub fn estimate_complexity(task_description: &str) -> u8 {
        let mut score: u8 = 0;

        // Length heuristic
        if task_description.len() > 100 {
            score += 1;
        }
        if task_description.len() > 500 {
            score += 1;
        }

        // Keyword heuristics
        let lower = task_description.to_lowercase();

        let high_complexity_words = [
            "refactor", "architect", "design", "optimize", "security",
            "algorithm", "database", "concurrent", "distributed", "scale",
            "debug", "investigate", "root cause", "performance", "critical",
        ];
        for word in &high_complexity_words {
            if lower.contains(word) {
                score += 2;
            }
        }

        let medium_complexity_words = [
            "implement", "create", "add", "build", "fix", "update",
            "test", "deploy", "review", "analyze",
        ];
        for word in &medium_complexity_words {
            if lower.contains(word) {
                score += 1;
            }
        }

        // Multi-step indicators
        let multi_step_words = ["and then", "first", "then", "also", "additionally", "finally"];
        for word in &multi_step_words {
            if lower.contains(word) {
                score += 1;
            }
        }

        score.min(10)
    }

    /// Get a profile for quick/simple tasks
    pub fn quick() -> InferenceProfile {
        InferenceProfile::new(
            CapabilityTier::Local,
            ThinkingLevel::Off,
            ContextClass::Small,
        )
    }

    /// Get a profile for standard tasks
    pub fn standard() -> InferenceProfile {
        InferenceProfile::new(
            CapabilityTier::Fast,
            ThinkingLevel::Minimal,
            ContextClass::Medium,
        )
    }

    /// Get a profile for complex tasks
    pub fn deep() -> InferenceProfile {
        InferenceProfile::new(
            CapabilityTier::Powerful,
            ThinkingLevel::High,
            ContextClass::Full,
        )
    }
}

// ─── Cost Tracking ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CostEntry {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub task: String,
    pub model: String,
    pub tokens_in: usize,
    pub tokens_out: usize,
    pub cost_usd: f64,
    pub latency_ms: u64,
    pub capability: CapabilityTier,
    pub thinking: ThinkingLevel,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct CostTracker {
    entries: Vec<CostEntry>,
    total_cost_usd: f64,
    total_tokens_in: usize,
    total_tokens_out: usize,
    total_latency_ms: u64,
}

impl CostTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&mut self, entry: CostEntry) {
        self.total_cost_usd += entry.cost_usd;
        self.total_tokens_in += entry.tokens_in;
        self.total_tokens_out += entry.tokens_out;
        self.total_latency_ms += entry.latency_ms;
        self.entries.push(entry);
    }

    pub fn total_cost(&self) -> f64 {
        self.total_cost_usd
    }

    pub fn total_tokens(&self) -> usize {
        self.total_tokens_in + self.total_tokens_out
    }

    pub fn avg_latency_ms(&self) -> u64 {
        if self.entries.is_empty() {
            0
        } else {
            self.total_latency_ms / self.entries.len() as u64
        }
    }

    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    pub fn recent(&self, n: usize) -> &[CostEntry] {
        let start = self.entries.len().saturating_sub(n);
        &self.entries[start..]
    }

    pub fn by_model(&self) -> std::collections::HashMap<String, (usize, f64)> {
        let mut map = std::collections::HashMap::new();
        for entry in &self.entries {
            let e = map.entry(entry.model.clone()).or_insert((0, 0.0));
            e.0 += entry.tokens_in + entry.tokens_out;
            e.1 += entry.cost_usd;
        }
        map
    }

    /// Save tracker to JSON file.
    pub fn save(&self, path: &std::path::Path) -> crate::Result<()> {
        let json = serde_json::to_string_pretty(self).map_err(|e| InferenceError::Config(e.to_string()))?;
        std::fs::write(path, json).map_err(|e| InferenceError::Config(e.to_string()))
    }

    /// Load tracker from JSON file. Returns empty tracker if file doesn't exist.
    pub fn load(path: &std::path::Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn summary(&self) -> String {
        format!(
            "Cost: ${:.4} | Tokens: {} in, {} out | Latency: avg {}ms | {} calls",
            self.total_cost_usd,
            self.total_tokens_in,
            self.total_tokens_out,
            self.avg_latency_ms(),
            self.entries.len()
        )
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_tier_models() {
        assert_eq!(CapabilityTier::Local.models().len(), 3);
        assert!(CapabilityTier::Local.models().contains(&"qwen3:4b"));
    }

    #[test]
    fn test_thinking_level_tokens() {
        assert_eq!(ThinkingLevel::Off.max_reasoning_tokens(), 0);
        assert_eq!(ThinkingLevel::High.max_reasoning_tokens(), 8192);
    }

    #[test]
    fn test_context_class() {
        assert_eq!(ContextClass::Small.max_messages(), 2);
        assert!(!ContextClass::Small.include_memory());
        assert!(ContextClass::Large.include_memory());
    }

    #[test]
    fn test_profile_cost() {
        let cheap = InferenceController::quick();
        let expensive = InferenceController::deep();
        assert!(cheap.estimated_cost_per_query() < expensive.estimated_cost_per_query());
    }

    #[test]
    fn test_profile_latency() {
        let quick = InferenceController::quick();
        let deep = InferenceController::deep();
        assert!(quick.estimated_latency_ms() < deep.estimated_latency_ms());
    }

    #[test]
    fn test_select_simple_task() {
        let profile = InferenceController::select_profile("hello");
        assert_eq!(profile.capability, CapabilityTier::Local);
        assert_eq!(profile.thinking, ThinkingLevel::Off);
    }

    #[test]
    fn test_select_complex_task() {
        let profile = InferenceController::select_profile(
            "Refactor the authentication system to support OAuth2, implement proper token rotation, and ensure security best practices are followed throughout the codebase",
        );
        assert!(
            profile.capability == CapabilityTier::Balanced
                || profile.capability == CapabilityTier::Powerful
        );
        assert!(
            profile.thinking == ThinkingLevel::Medium
                || profile.thinking == ThinkingLevel::High
        );
    }

    #[test]
    fn test_complexity_estimate() {
        assert!(InferenceController::estimate_complexity("hi") < 3);
        assert!(
            InferenceController::estimate_complexity(
                "Refactor the distributed authentication system with concurrent token rotation and security audit"
            ) >= 5
        );
    }

    #[test]
    fn test_profiles() {
        let q = InferenceController::quick();
        assert_eq!(q.capability, CapabilityTier::Local);

        let s = InferenceController::standard();
        assert_eq!(s.capability, CapabilityTier::Fast);

        let d = InferenceController::deep();
        assert_eq!(d.capability, CapabilityTier::Powerful);
    }

    #[test]
    fn test_all_states_roundtrip() {
        let cap = CapabilityTier::Local;
        let json = serde_json::to_string(&cap).unwrap();
        let parsed: CapabilityTier = serde_json::from_str(&json).unwrap();
        assert_eq!(cap, parsed);
    }

    #[test]
    fn test_context_class_history() {
        assert!(!ContextClass::Small.include_history());
        assert!(ContextClass::Medium.include_history());
        assert!(ContextClass::Large.include_history());
    }

    #[test]
    fn test_cost_tracker() {
        let mut tracker = CostTracker::new();
tracker.record(CostEntry {
            timestamp: chrono::Utc::now(),
            task: "task1".into(),
            model: "qwen3:8b".into(),
            tokens_in: 100,
            tokens_out: 200,
            cost_usd: 0.001,
            latency_ms: 500,
            capability: CapabilityTier::Fast,
            thinking: ThinkingLevel::Minimal,
        });

        tracker.record(CostEntry {
            timestamp: chrono::Utc::now(),
            task: "task2".into(),
            model: "qwen3:4b".into(),
            tokens_in: 50,
            tokens_out: 100,
            cost_usd: 0.0005,
            latency_ms: 200,
            capability: CapabilityTier::Local,
            thinking: ThinkingLevel::Off,
        });
        assert_eq!(tracker.entry_count(), 2);
        assert!((tracker.total_cost() - 0.0015).abs() < 0.0001);
        assert_eq!(tracker.total_tokens(), 450);
        assert!(tracker.avg_latency_ms() > 0);
    }

    #[test]
    fn test_cost_tracker_by_model() {
        let mut tracker = CostTracker::new();
        tracker.record(CostEntry {
            timestamp: chrono::Utc::now(),
            task: "t1".into(),
            model: "qwen3:8b".into(),
            tokens_in: 100,
            tokens_out: 200,
            cost_usd: 0.001,
            latency_ms: 500,
            capability: CapabilityTier::Fast,
            thinking: ThinkingLevel::Minimal,
        });
        tracker.record(CostEntry {
            timestamp: chrono::Utc::now(),
            task: "t2".into(),
            model: "qwen3:8b".into(),
            tokens_in: 50,
            tokens_out: 100,
            cost_usd: 0.0005,
            latency_ms: 200,
            capability: CapabilityTier::Fast,
            thinking: ThinkingLevel::Minimal,
        });
        tracker.record(CostEntry {
            timestamp: chrono::Utc::now(),
            task: "t3".into(),
            model: "qwen3:4b".into(),
            tokens_in: 50,
            tokens_out: 100,
            cost_usd: 0.0002,
            latency_ms: 100,
            capability: CapabilityTier::Local,
            thinking: ThinkingLevel::Off,
        });

        let by_model = tracker.by_model();
        assert_eq!(by_model.len(), 2);
        let (tokens, cost) = by_model.get("qwen3:8b").unwrap();
        assert_eq!(*tokens, 450);
        assert!((*cost - 0.0015).abs() < 0.0001);
    }

    #[test]
    fn test_cost_tracker_summary() {
        let mut tracker = CostTracker::new();
        tracker.record(CostEntry {
            timestamp: chrono::Utc::now(),
            task: "t1".into(),
            model: "model".into(),
            tokens_in: 100,
            tokens_out: 200,
            cost_usd: 0.001,
            latency_ms: 500,
            capability: CapabilityTier::Fast,
            thinking: ThinkingLevel::Minimal,
        });
        let summary = tracker.summary();
        assert!(summary.contains("$0.0010"));
        assert!(summary.contains("100 in"));
        assert!(summary.contains("200 out"));
    }
}
