//! soullink-moe — Mixture of Experts for task routing.
//!
//! Classifies incoming tasks and routes them to the most appropriate
//! specialized model (coding, science, creative, etc.).
//!
//! # Architecture
//!
//! ```text
//!   User Query
//!       │
//!       ▼
//!   TaskClassifier  ──(keyword + complexity)──►  TaskDomain
//!       │
//!       ▼
//!   ExpertRouter    ──(domain + VRAM + load)──►  ExpertModel
//!       │
//!       ▼
//!   OllamaInference ──(model + prompt)──►  Response
//! ```

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::info;

// ── Task Classification ─────────────────────────────────────────────────

/// Domain of a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TaskDomain {
    /// Code generation, debugging, refactoring.
    Coding,
    /// Physics, math, chemistry, research.
    Science,
    /// Creative writing, brainstorming, metaphor.
    Creative,
    /// Trading, blockchain, finance.
    Finance,
    /// Logic, optimization, planning.
    Reasoning,
    /// General conversation, summarization.
    General,
}

impl TaskDomain {
    /// All domains.
    pub fn all() -> &'static [TaskDomain] {
        &[
            TaskDomain::Coding,
            TaskDomain::Science,
            TaskDomain::Creative,
            TaskDomain::Finance,
            TaskDomain::Reasoning,
            TaskDomain::General,
        ]
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            TaskDomain::Coding => "coding",
            TaskDomain::Science => "science",
            TaskDomain::Creative => "creative",
            TaskDomain::Finance => "finance",
            TaskDomain::Reasoning => "reasoning",
            TaskDomain::General => "general",
        }
    }
}

impl std::fmt::Display for TaskDomain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Classification result for a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskClassification {
    /// Primary domain.
    pub domain: TaskDomain,
    /// Confidence score [0.0, 1.0].
    pub confidence: f32,
    /// Complexity score [0, 100].
    pub complexity: u8,
    /// Detected keywords that drove the classification.
    pub keywords: Vec<String>,
}

// ── Expert Model ────────────────────────────────────────────────────────

/// A specialized model (expert) for a domain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpertModel {
    /// Expert name (e.g., "coder", "scientist").
    pub name: String,
    /// Ollama model identifier (e.g., "qwen2.5-coder:3b").
    pub model: String,
    /// Quantization level (e.g., "Q4_K_M").
    pub quantization: String,
    /// Domains this expert handles.
    pub domains: Vec<TaskDomain>,
    /// Priority (higher = prefer this expert).
    pub priority: i32,
    /// Estimated VRAM usage in MiB.
    pub vram_mib: u64,
    /// Whether this expert is currently loaded.
    pub loaded: bool,
    /// Total requests served.
    pub request_count: u64,
    /// Average response time in ms.
    pub avg_latency_ms: f64,
}

// ── Expert Router ───────────────────────────────────────────────────────

/// Routes tasks to the best expert model.
pub struct ExpertRouter {
    experts: DashMap<String, ExpertModel>,
    /// Per-domain request counters.
    domain_counts: DashMap<TaskDomain, AtomicU64>,
}

impl ExpertRouter {
    pub fn new() -> Self {
        Self {
            experts: DashMap::new(),
            domain_counts: DashMap::new(),
        }
    }

    /// Register an expert model.
    pub fn register(&self, expert: ExpertModel) {
        info!(
            "MoE: registered expert '{}' for domains {:?}",
            expert.name, expert.domains
        );
        self.experts.insert(expert.name.clone(), expert);
    }

    /// Route a classified task to the best expert.
    pub fn route(&self, classification: &TaskClassification) -> Option<ExpertModel> {
        let mut candidates: Vec<ExpertModel> = self
            .experts
            .iter()
            .filter(|e| e.value().domains.contains(&classification.domain))
            .map(|e| e.value().clone())
            .collect();

        if candidates.is_empty() {
            // Fallback to any general expert
            candidates = self
                .experts
                .iter()
                .filter(|e| e.value().domains.contains(&TaskDomain::General))
                .map(|e| e.value().clone())
                .collect();
        }

        if candidates.is_empty() {
            return None;
        }

        // Sort by: loaded first, then priority, then request count (least loaded)
        candidates.sort_by(|a, b| {
            b.loaded
                .cmp(&a.loaded)
                .then(b.priority.cmp(&a.priority))
                .then(a.request_count.cmp(&b.request_count))
        });

        let chosen = candidates.into_iter().next()?;

        // Increment domain counter
        self.domain_counts
            .entry(classification.domain)
            .or_default()
            .fetch_add(1, Ordering::Relaxed);

        // Increment expert request count
        if let Some(mut expert) = self.experts.get_mut(&chosen.name) {
            expert.request_count += 1;
        }

        Some(chosen)
    }

    /// Get all registered experts.
    pub fn all_experts(&self) -> Vec<ExpertModel> {
        self.experts.iter().map(|e| e.value().clone()).collect()
    }

    /// Get experts for a specific domain.
    pub fn experts_for_domain(&self, domain: TaskDomain) -> Vec<ExpertModel> {
        self.experts
            .iter()
            .filter(|e| e.value().domains.contains(&domain))
            .map(|e| e.value().clone())
            .collect()
    }

    /// Get request counts per domain.
    pub fn domain_stats(&self) -> HashMap<String, u64> {
        self.domain_counts
            .iter()
            .map(|e| (e.key().to_string(), e.value().load(Ordering::Relaxed)))
            .collect()
    }
}

impl Default for ExpertRouter {
    fn default() -> Self {
        Self::new()
    }
}

// ── Task Classifier ─────────────────────────────────────────────────────

/// Keyword-based task classifier.
pub struct TaskClassifier {
    /// Domain keywords for classification.
    keywords: HashMap<TaskDomain, Vec<String>>,
    /// Complexity keywords (increase complexity score).
    complexity_keywords: Vec<String>,
}

impl TaskClassifier {
    pub fn new() -> Self {
        let mut keywords = HashMap::new();

        keywords.insert(
            TaskDomain::Coding,
            vec![
                "code",
                "function",
                "rust",
                "python",
                "debug",
                "refactor",
                "compile",
                "algorithm",
                "implement",
                "api",
                "struct",
                "trait",
                "impl",
                "test",
                "bug",
                "fix",
                "error",
                "type",
                "module",
                "crate",
                "dependency",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
        );

        keywords.insert(
            TaskDomain::Science,
            vec![
                "physics",
                "math",
                "chemistry",
                "equation",
                "formula",
                "theorem",
                "prove",
                "derivative",
                "integral",
                "quantum",
                "entropy",
                "force",
                "energy",
                "experiment",
                "hypothesis",
                "research",
                "paper",
                "arxiv",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
        );

        keywords.insert(
            TaskDomain::Creative,
            vec![
                "write",
                "story",
                "poem",
                "creative",
                "imagine",
                "metaphor",
                "brainstorm",
                "idea",
                "design",
                "art",
                "music",
                "color",
                "emotion",
                "narrative",
                "character",
                "dialogue",
                "scene",
                "novel",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
        );

        keywords.insert(
            TaskDomain::Finance,
            vec![
                "trading",
                "stock",
                "crypto",
                "bitcoin",
                "defi",
                "portfolio",
                "risk",
                "return",
                "market",
                "price",
                "chart",
                "technical analysis",
                "fundamental",
                "dividend",
                "yield",
                "hedge",
                "position",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
        );

        keywords.insert(
            TaskDomain::Reasoning,
            vec![
                "analyze",
                "plan",
                "strategy",
                "optimize",
                "logic",
                "deduce",
                "compare",
                "evaluate",
                "decide",
                "tradeoff",
                "pros",
                "cons",
                "step by step",
                "reason",
                "explain why",
                "because",
                "therefore",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
        );

        Self {
            keywords,
            complexity_keywords: vec![
                "complex",
                "advanced",
                "detailed",
                "comprehensive",
                "thorough",
                "in-depth",
                "elaborate",
                "systematic",
                "multi-step",
                "edge case",
                "optimize",
                "performance",
                "scalable",
                "architecture",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
        }
    }

    /// Classify a task based on its text content.
    pub fn classify(&self, text: &str) -> TaskClassification {
        let lower = text.to_lowercase();
        let words: Vec<&str> = lower.split_whitespace().collect();

        // Score each domain by keyword matches
        let mut scores: HashMap<TaskDomain, f32> = HashMap::new();
        let mut matched_keywords: Vec<String> = Vec::new();

        for (domain, domain_keywords) in &self.keywords {
            let mut score = 0.0f32;
            for kw in domain_keywords {
                if lower.contains(kw.as_str()) {
                    score += 1.0;
                    matched_keywords.push(kw.clone());
                }
            }
            // Normalize by number of keywords
            if !domain_keywords.is_empty() {
                score /= domain_keywords.len() as f32;
            }
            scores.insert(*domain, score);
        }

        // Find the best domain
        let (best_domain, best_score) = scores
            .iter()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(d, s)| (*d, *s))
            .unwrap_or((TaskDomain::General, 0.0));

        // If no strong match, default to General
        let domain = if best_score < 0.02 {
            TaskDomain::General
        } else {
            best_domain
        };

        // Calculate complexity [0, 100]
        let complexity_base = (words.len() as f32 / 10.0 * 20.0).min(60.0);
        let complexity_bonus = self
            .complexity_keywords
            .iter()
            .filter(|kw| lower.contains(kw.as_str()))
            .count() as f32
            * 10.0;
        let complexity = (complexity_base + complexity_bonus).min(100.0) as u8;

        // Confidence based on score spread
        let max_score = best_score;
        let second_best = scores
            .values()
            .filter(|s| **s > 0.0)
            .copied()
            .fold(0.0f32, f32::max);
        let confidence = if max_score > 0.0 {
            (max_score - second_best / max_score).clamp(0.3, 1.0)
        } else {
            0.1
        };

        TaskClassification {
            domain,
            confidence,
            complexity,
            keywords: matched_keywords,
        }
    }
}

impl Default for TaskClassifier {
    fn default() -> Self {
        Self::new()
    }
}

// ── MoE Router (top-level) ──────────────────────────────────────────────

/// Top-level MoE router combining classification and routing.
pub struct MoERouter {
    classifier: TaskClassifier,
    router: ExpertRouter,
}

impl MoERouter {
    pub fn new() -> Self {
        Self {
            classifier: TaskClassifier::new(),
            router: ExpertRouter::new(),
        }
    }

    /// Register an expert.
    pub fn register_expert(&self, expert: ExpertModel) {
        self.router.register(expert);
    }

    /// Classify and route a task.
    pub fn classify_and_route(&self, text: &str) -> (TaskClassification, Option<ExpertModel>) {
        let classification = self.classifier.classify(text);
        let expert = self.router.route(&classification);
        (classification, expert)
    }

    /// Get the classifier.
    pub fn classifier(&self) -> &TaskClassifier {
        &self.classifier
    }

    /// Get the router.
    pub fn router(&self) -> &ExpertRouter {
        &self.router
    }
}

impl Default for MoERouter {
    fn default() -> Self {
        Self::new()
    }
}

/// Create a MoERouter with real Ollama model experts registered.
pub fn moe_with_ollama_experts() -> MoERouter {
    let moe = MoERouter::new();

    let experts = vec![
        ExpertModel {
            name: "coder".into(),
            model: "qwen2.5-coder:3b".into(),
            quantization: "Q4_K_M".into(),
            domains: vec![TaskDomain::Coding],
            priority: 10,
            vram_mib: 2000,
            loaded: true,
            request_count: 0,
            avg_latency_ms: 0.0,
        },
        ExpertModel {
            name: "scientist".into(),
            model: "gemma4:31b".into(),
            quantization: "Q4_K_M".into(),
            domains: vec![TaskDomain::Science],
            priority: 8,
            vram_mib: 8000,
            loaded: false,
            request_count: 0,
            avg_latency_ms: 0.0,
        },
        ExpertModel {
            name: "creative".into(),
            model: "qwen2.5:7b".into(),
            quantization: "Q4_K_M".into(),
            domains: vec![TaskDomain::Creative, TaskDomain::General],
            priority: 7,
            vram_mib: 4000,
            loaded: true,
            request_count: 0,
            avg_latency_ms: 0.0,
        },
        ExpertModel {
            name: "reasoner".into(),
            model: "deepseek-r1:8b".into(),
            quantization: "Q4_K_M".into(),
            domains: vec![TaskDomain::Reasoning],
            priority: 9,
            vram_mib: 5000,
            loaded: false,
            request_count: 0,
            avg_latency_ms: 0.0,
        },
        ExpertModel {
            name: "finance".into(),
            model: "qwen2.5:3b".into(),
            quantization: "Q4_K_M".into(),
            domains: vec![TaskDomain::Finance],
            priority: 6,
            vram_mib: 2000,
            loaded: false,
            request_count: 0,
            avg_latency_ms: 0.0,
        },
    ];

    for expert in experts {
        moe.register_expert(expert);
    }

    moe
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_coding() {
        let classifier = TaskClassifier::new();
        let c = classifier.classify("Write a Rust function to parse JSON");
        assert_eq!(c.domain, TaskDomain::Coding);
        assert!(c.confidence > 0.1);
        assert!(c.keywords.contains(&"rust".to_string()));
    }

    #[test]
    fn test_classify_science() {
        let classifier = TaskClassifier::new();
        let c = classifier.classify("Explain the Schrodinger equation in quantum physics");
        assert_eq!(c.domain, TaskDomain::Science);
        assert!(c.keywords.contains(&"quantum".to_string()));
    }

    #[test]
    fn test_classify_creative() {
        let classifier = TaskClassifier::new();
        let c = classifier.classify("Write a poem about the sunset over the ocean");
        assert_eq!(c.domain, TaskDomain::Creative);
    }

    #[test]
    fn test_classify_general() {
        let classifier = TaskClassifier::new();
        let c = classifier.classify("Hello, how are you?");
        assert_eq!(c.domain, TaskDomain::General);
    }

    #[test]
    fn test_complexity_scoring() {
        let classifier = TaskClassifier::new();
        let simple = classifier.classify("Fix the bug");
        let complex = classifier.classify(
            "Implement a comprehensive, multi-step optimization for the complex architecture with edge cases",
        );
        assert!(complex.complexity > simple.complexity);
    }

    #[test]
    fn test_expert_routing() {
        let router = ExpertRouter::new();

        router.register(ExpertModel {
            name: "coder".into(),
            model: "qwen2.5-coder:3b".into(),
            quantization: "Q4_K_M".into(),
            domains: vec![TaskDomain::Coding],
            priority: 10,
            vram_mib: 2000,
            loaded: true,
            request_count: 0,
            avg_latency_ms: 0.0,
        });

        router.register(ExpertModel {
            name: "scientist".into(),
            model: "gemma4:31b".into(),
            quantization: "Q4_K_M".into(),
            domains: vec![TaskDomain::Science],
            priority: 5,
            vram_mib: 8000,
            loaded: false,
            request_count: 0,
            avg_latency_ms: 0.0,
        });

        let classification = TaskClassification {
            domain: TaskDomain::Coding,
            confidence: 0.8,
            complexity: 30,
            keywords: vec!["rust".into()],
        };

        let expert = router.route(&classification).unwrap();
        assert_eq!(expert.name, "coder");
    }

    #[test]
    fn test_moe_router_end_to_end() {
        let moe = MoERouter::new();

        moe.register_expert(ExpertModel {
            name: "coder".into(),
            model: "qwen2.5-coder:3b".into(),
            quantization: "Q4_K_M".into(),
            domains: vec![TaskDomain::Coding],
            priority: 10,
            vram_mib: 2000,
            loaded: true,
            request_count: 0,
            avg_latency_ms: 0.0,
        });

        let (classification, expert) = moe.classify_and_route("Debug this Rust code that crashes");
        assert_eq!(classification.domain, TaskDomain::Coding);
        assert!(expert.is_some());
        assert_eq!(expert.unwrap().name, "coder");
    }

    #[test]
    fn test_domain_stats() {
        let router = ExpertRouter::new();
        router.register(ExpertModel {
            name: "coder".into(),
            model: "m".into(),
            quantization: "q".into(),
            domains: vec![TaskDomain::Coding],
            priority: 1,
            vram_mib: 100,
            loaded: true,
            request_count: 0,
            avg_latency_ms: 0.0,
        });

        let c = TaskClassification {
            domain: TaskDomain::Coding,
            confidence: 0.9,
            complexity: 10,
            keywords: vec![],
        };

        router.route(&c);
        router.route(&c);
        let stats = router.domain_stats();
        assert_eq!(stats.get("coding"), Some(&2));
    }

    #[test]
    fn test_moe_with_ollama_experts() {
        let moe = moe_with_ollama_experts();
        let experts = moe.router().all_experts();
        assert_eq!(experts.len(), 5);
        assert!(experts.iter().any(|e| e.name == "coder"));
        assert!(experts.iter().any(|e| e.name == "scientist"));
        assert!(experts.iter().any(|e| e.name == "creative"));
        assert!(experts.iter().any(|e| e.name == "reasoner"));
        assert!(experts.iter().any(|e| e.name == "finance"));

        let (classification, expert) =
            moe.classify_and_route("Write a Rust function to parse JSON");
        assert_eq!(classification.domain, TaskDomain::Coding);
        assert_eq!(expert.unwrap().name, "coder");
    }
}
