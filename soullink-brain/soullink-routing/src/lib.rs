//! Smart LLM routing with 13-dimension complexity scoring.
//!
//! Ported from SoulSystem Integration's `smart_routing.rs` for SoulLink's organ mesh.
//! Routes requests to cheap or primary models based on task complexity,
//! with pattern overrides for fast-path routing.

use std::sync::atomic::{AtomicU64, Ordering};

use regex::Regex;
use serde::{Deserialize, Serialize};

use soullink_circuit::{CircuitBreaker, CircuitBreakerConfig, CircuitState};

// ---------------------------------------------------------------------------
// Complexity tiers & scoring
// ---------------------------------------------------------------------------

/// Complexity tier produced by the 13-dimension scorer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Tier {
    Flash,
    Standard,
    Pro,
    Frontier,
}

impl Tier {
    pub fn from_score(score: u32) -> Self {
        match score {
            0..=15 => Tier::Flash,
            16..=40 => Tier::Standard,
            41..=65 => Tier::Pro,
            _ => Tier::Frontier,
        }
    }

    pub fn to_score(self) -> u32 {
        match self {
            Tier::Flash => 8,
            Tier::Standard => 28,
            Tier::Pro => 52,
            Tier::Frontier => 80,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Tier::Flash => "flash",
            Tier::Standard => "standard",
            Tier::Pro => "pro",
            Tier::Frontier => "frontier",
        }
    }
}

// ---------------------------------------------------------------------------
// 13-Dimension Complexity Scorer
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplexityScore {
    pub reasoning: u32,
    pub code_complexity: u32,
    pub multi_step: u32,
    pub domain_specific: u32,
    pub creativity: u32,
    pub precision: u32,
    pub safety_critical: u32,
    pub context_depth: u32,
    pub tool_use: u32,
    pub ambiguity: u32,
    pub length: u32,
    pub technical_depth: u32,
    pub novelty: u32,
    pub total: u32,
    pub tier: Tier,
}

/// Weights for each dimension (sum = 100).
const DIMENSION_WEIGHTS: [(Dimension, u32); 13] = [
    (Dimension::Reasoning, 15),
    (Dimension::CodeComplexity, 12),
    (Dimension::MultiStep, 10),
    (Dimension::DomainSpecific, 8),
    (Dimension::Creativity, 8),
    (Dimension::Precision, 10),
    (Dimension::SafetyCritical, 12),
    (Dimension::ContextDepth, 6),
    (Dimension::ToolUse, 5),
    (Dimension::Ambiguity, 4),
    (Dimension::Length, 3),
    (Dimension::TechnicalDepth, 4),
    (Dimension::Novelty, 3),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Dimension {
    Reasoning,
    CodeComplexity,
    MultiStep,
    DomainSpecific,
    Creativity,
    Precision,
    SafetyCritical,
    ContextDepth,
    ToolUse,
    Ambiguity,
    Length,
    TechnicalDepth,
    Novelty,
}

pub struct ComplexityScorer {
    patterns: Vec<PatternOverride>,
}

struct PatternOverride {
    regex: Regex,
    tier: Tier,
    #[allow(dead_code)]
    name: String,
}

impl ComplexityScorer {
    pub fn new() -> Self {
        let patterns = vec![
            PatternOverride {
                regex: Regex::new(r"(?i)^(hi|hello|hey|salut|bonjour)\b").unwrap(),
                tier: Tier::Flash,
                name: "greeting".into(),
            },
            PatternOverride {
                regex: Regex::new(
                    r"(?i)(security\s+audit|penetration\s+test|vulnerability\s+scan)",
                )
                .unwrap(),
                tier: Tier::Frontier,
                name: "security_audit".into(),
            },
            PatternOverride {
                regex: Regex::new(r"(?i)(review\s+this\s+code|code\s+review|refactor)").unwrap(),
                tier: Tier::Pro,
                name: "code_review".into(),
            },
            PatternOverride {
                regex: Regex::new(r"(?i)(what\s+is|define|explain\s+simply)").unwrap(),
                tier: Tier::Flash,
                name: "simple_question".into(),
            },
            PatternOverride {
                regex: Regex::new(r"(?i)(step\s+by\s+step|first.*then.*finally)").unwrap(),
                tier: Tier::Pro,
                name: "multi_step_hint".into(),
            },
        ];

        Self { patterns }
    }

    /// Score a prompt across 13 dimensions.
    pub fn score(&self, prompt: &str) -> ComplexityScore {
        // Check pattern overrides first
        for pat in &self.patterns {
            if pat.regex.is_match(prompt) {
                tracing::debug!(pattern = %pat.name, tier = ?pat.tier, "pattern override matched");
                let s = pat.tier.to_score();
                return ComplexityScore {
                    reasoning: if pat.tier == Tier::Flash { 5 } else { s },
                    code_complexity: 5,
                    multi_step: if pat.tier >= Tier::Pro { 10 } else { 5 },
                    domain_specific: 5,
                    creativity: 5,
                    precision: 5,
                    safety_critical: if pat.tier == Tier::Frontier { 15 } else { 5 },
                    context_depth: 5,
                    tool_use: 5,
                    ambiguity: 5,
                    length: self.score_length(prompt),
                    technical_depth: if pat.tier >= Tier::Pro { 8 } else { 5 },
                    novelty: 5,
                    total: s,
                    tier: pat.tier,
                };
            }
        }

        // Full 13-dimension scoring
        let scores = [
            self.score_reasoning(prompt),
            self.score_code_complexity(prompt),
            self.score_multi_step(prompt),
            self.score_domain_specific(prompt),
            self.score_creativity(prompt),
            self.score_precision(prompt),
            self.score_safety_critical(prompt),
            self.score_context_depth(prompt),
            self.score_tool_use(prompt),
            self.score_ambiguity(prompt),
            self.score_length(prompt),
            self.score_technical_depth(prompt),
            self.score_novelty(prompt),
        ];

        let total: u32 = DIMENSION_WEIGHTS
            .iter()
            .zip(scores.iter())
            .map(|((_, w), s)| (*s * *w) / 100)
            .sum();

        let tier = Tier::from_score(total);

        ComplexityScore {
            reasoning: scores[0],
            code_complexity: scores[1],
            multi_step: scores[2],
            domain_specific: scores[3],
            creativity: scores[4],
            precision: scores[5],
            safety_critical: scores[6],
            context_depth: scores[7],
            tool_use: scores[8],
            ambiguity: scores[9],
            length: scores[10],
            technical_depth: scores[11],
            novelty: scores[12],
            total,
            tier,
        }
    }

    // --- Dimension scorers (0-100 each) ---

    fn score_reasoning(&self, prompt: &str) -> u32 {
        let mut score = 10u32;
        let lower = prompt.to_lowercase();
        if lower.contains("why") || lower.contains("explain") {
            score += 15;
        }
        if lower.contains("analyze") || lower.contains("compare") {
            score += 20;
        }
        if lower.contains("prove") || lower.contains("derive") {
            score += 25;
        }
        if lower.contains("reasoning") || lower.contains("logic") {
            score += 15;
        }
        score.min(100)
    }

    fn score_code_complexity(&self, prompt: &str) -> u32 {
        let mut score = 10u32;
        if prompt.contains("fn ") || prompt.contains("function") {
            score += 20;
        }
        if prompt.contains("impl ") || prompt.contains("class ") {
            score += 25;
        }
        if prompt.contains("async") || prompt.contains("await") {
            score += 15;
        }
        if prompt.contains("unsafe") || prompt.contains("trait ") {
            score += 20;
        }
        score.min(100)
    }

    fn score_multi_step(&self, prompt: &str) -> u32 {
        let lower = prompt.to_lowercase();
        let step_words = ["first", "then", "after", "next", "finally", "step"];
        let count = step_words.iter().filter(|w| lower.contains(*w)).count();
        10 + (count as u32 * 20).min(70)
    }

    fn score_domain_specific(&self, prompt: &str) -> u32 {
        let mut score = 10u32;
        let lower = prompt.to_lowercase();
        let domains = [
            "cryptograph",
            "quantum",
            "neural",
            "compil",
            "gpu",
            "assembly",
        ];
        for d in &domains {
            if lower.contains(d) {
                score += 30;
                break;
            }
        }
        score.min(100)
    }

    fn score_creativity(&self, prompt: &str) -> u32 {
        let lower = prompt.to_lowercase();
        let creative = [
            "create",
            "design",
            "imagine",
            "invent",
            "brainstorm",
            "story",
        ];
        let count = creative.iter().filter(|w| lower.contains(*w)).count();
        10 + (count as u32 * 25).min(65)
    }

    fn score_precision(&self, prompt: &str) -> u32 {
        let lower = prompt.to_lowercase();
        let precise = [
            "exactly",
            "precise",
            "specific",
            "calculate",
            "compute",
            "measure",
        ];
        let count = precise.iter().filter(|w| lower.contains(*w)).count();
        10 + (count as u32 * 25).min(65)
    }

    fn score_safety_critical(&self, prompt: &str) -> u32 {
        let lower = prompt.to_lowercase();
        let safety = [
            "security",
            "vulnerability",
            "exploit",
            "inject",
            "sanitize",
            "escape",
        ];
        let count = safety.iter().filter(|w| lower.contains(*w)).count();
        5 + (count as u32 * 30).min(85)
    }

    fn score_context_depth(&self, prompt: &str) -> u32 {
        let words = prompt.split_whitespace().count();
        if words < 20 {
            10
        } else if words < 50 {
            25
        } else if words < 100 {
            50
        } else if words < 200 {
            70
        } else {
            90
        }
    }

    fn score_tool_use(&self, prompt: &str) -> u32 {
        let lower = prompt.to_lowercase();
        let tools = ["curl", "git", "docker", "kubectl", "ssh", "terraform"];
        let count = tools.iter().filter(|w| lower.contains(*w)).count();
        10 + (count as u32 * 25).min(65)
    }

    fn score_ambiguity(&self, prompt: &str) -> u32 {
        let lower = prompt.to_lowercase();
        let ambiguous = [
            "maybe", "might", "perhaps", "could be", "i think", "roughly",
        ];
        let count = ambiguous.iter().filter(|w| lower.contains(*w)).count();
        5 + (count as u32 * 20).min(60)
    }

    fn score_length(&self, prompt: &str) -> u32 {
        let words = prompt.split_whitespace().count() as u32;
        if words < 10 {
            5
        } else if words < 30 {
            15
        } else if words < 60 {
            30
        } else if words < 120 {
            50
        } else {
            70
        }
    }

    fn score_technical_depth(&self, prompt: &str) -> u32 {
        let lower = prompt.to_lowercase();
        let tech = [
            "algorithm",
            "complexity",
            "big-o",
            "runtime",
            "memory",
            "latency",
            "throughput",
        ];
        let count = tech.iter().filter(|w| lower.contains(*w)).count();
        10 + (count as u32 * 20).min(65)
    }

    fn score_novelty(&self, prompt: &str) -> u32 {
        let lower = prompt.to_lowercase();
        let novel = ["never before", "novel", "original", "unique", "innovative"];
        let count = novel.iter().filter(|w| lower.contains(*w)).count();
        5 + (count as u32 * 25).min(70)
    }
}

impl Default for ComplexityScorer {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Route Decision
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteDecision {
    pub tier: Tier,
    pub score: u32,
    pub provider: ProviderChoice,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderChoice {
    Cheap,
    Primary,
    CircuitBreakerFallback,
}

// ---------------------------------------------------------------------------
// Smart Router
// ---------------------------------------------------------------------------

pub struct SmartRouter {
    scorer: ComplexityScorer,
    primary_name: String,
    cheap_name: String,
    cheap_circuit: CircuitBreaker,
    primary_circuit: CircuitBreaker,
    total_routed: AtomicU64,
    cheap_routed: AtomicU64,
}

impl SmartRouter {
    pub fn new(primary: &str, cheap: &str) -> Self {
        let cheap_circuit = CircuitBreaker::with_config(CircuitBreakerConfig {
            failure_threshold: 3,
            recovery_timeout: std::time::Duration::from_secs(30),
            half_open_successes_needed: 2,
            call_timeout: std::time::Duration::from_secs(60),
        });

        let primary_circuit = CircuitBreaker::with_config(CircuitBreakerConfig {
            failure_threshold: 5,
            recovery_timeout: std::time::Duration::from_secs(60),
            half_open_successes_needed: 2,
            call_timeout: std::time::Duration::from_secs(120),
        });

        Self {
            scorer: ComplexityScorer::new(),
            primary_name: primary.to_string(),
            cheap_name: cheap.to_string(),
            cheap_circuit,
            primary_circuit,
            total_routed: AtomicU64::new(0),
            cheap_routed: AtomicU64::new(0),
        }
    }

    /// Route a prompt to the appropriate provider.
    pub async fn route(&self, prompt: &str) -> RouteDecision {
        let score = self.scorer.score(prompt);
        self.total_routed.fetch_add(1, Ordering::Relaxed);

        let cheap_state = self.cheap_circuit.state().await;
        let primary_state = self.primary_circuit.state().await;

        // Circuit breaker overrides
        if score.tier <= Tier::Standard && cheap_state == CircuitState::Closed {
            self.cheap_routed.fetch_add(1, Ordering::Relaxed);
            return RouteDecision {
                tier: score.tier,
                score: score.total,
                provider: ProviderChoice::Cheap,
                reason: format!(
                    "tier={:?}, score={}, cheap available → {}",
                    score.tier, score.total, self.cheap_name
                ),
            };
        }

        // If cheap is open and we'd normally route there, cascade to primary
        if score.tier <= Tier::Standard
            && cheap_state == CircuitState::Open
            && primary_state == CircuitState::Closed
        {
            return RouteDecision {
                tier: score.tier,
                score: score.total,
                provider: ProviderChoice::CircuitBreakerFallback,
                reason: format!(
                    "tier={:?}, score={}, cheap circuit OPEN → cascade to {}",
                    score.tier, score.total, self.primary_name
                ),
            };
        }

        // Default: primary model
        RouteDecision {
            tier: score.tier,
            score: score.total,
            provider: ProviderChoice::Primary,
            reason: format!(
                "tier={:?}, score={} → {}",
                score.tier, score.total, self.primary_name
            ),
        }
    }

    /// Record a successful call to a provider.
    pub async fn record_success(&self, provider: ProviderChoice) {
        match provider {
            ProviderChoice::Cheap | ProviderChoice::CircuitBreakerFallback => {
                self.cheap_circuit.record_success().await;
            }
            ProviderChoice::Primary => {
                self.primary_circuit.record_success().await;
            }
        }
    }

    /// Record a failed call to a provider.
    pub async fn record_failure(&self, provider: ProviderChoice) {
        match provider {
            ProviderChoice::Cheap | ProviderChoice::CircuitBreakerFallback => {
                self.cheap_circuit.record_failure().await;
            }
            ProviderChoice::Primary => {
                self.primary_circuit.record_failure().await;
            }
        }
    }

    /// Get routing statistics.
    pub async fn stats(&self) -> RouterStats {
        let total = self.total_routed.load(Ordering::Relaxed);
        let cheap = self.cheap_routed.load(Ordering::Relaxed);
        RouterStats {
            total_requests: total,
            cheap_requests: cheap,
            primary_requests: total - cheap,
            cheap_circuit: self.cheap_circuit.stats().await,
            primary_circuit: self.primary_circuit.stats().await,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct RouterStats {
    pub total_requests: u64,
    pub cheap_requests: u64,
    pub primary_requests: u64,
    pub cheap_circuit: soullink_circuit::CircuitStats,
    pub primary_circuit: soullink_circuit::CircuitStats,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier_from_score() {
        assert_eq!(Tier::from_score(5), Tier::Flash);
        assert_eq!(Tier::from_score(20), Tier::Standard);
        assert_eq!(Tier::from_score(50), Tier::Pro);
        assert_eq!(Tier::from_score(80), Tier::Frontier);
    }

    #[test]
    fn greeting_is_flash() {
        let scorer = ComplexityScorer::new();
        let score = scorer.score("Hello, how are you?");
        assert_eq!(score.tier, Tier::Flash);
        assert!(score.total < 20);
    }

    #[test]
    fn security_audit_is_frontier() {
        let scorer = ComplexityScorer::new();
        let score = scorer.score("Please run a security audit on this codebase");
        assert_eq!(score.tier, Tier::Frontier);
    }

    #[test]
    fn code_review_is_pro() {
        let scorer = ComplexityScorer::new();
        let score = scorer.score("Can you review this code and suggest improvements?");
        assert!(score.tier >= Tier::Pro);
    }

    #[test]
    fn simple_question_is_flash() {
        let scorer = ComplexityScorer::new();
        let score = scorer.score("What is Rust?");
        assert!(score.tier <= Tier::Standard);
    }

    #[tokio::test]
    async fn router_routes_flash_to_cheap() {
        let router = SmartRouter::new("gemma4:31b", "qwen3:4b");
        let decision = router.route("Hello!").await;
        assert_eq!(decision.provider, ProviderChoice::Cheap);
    }

    #[tokio::test]
    async fn router_routes_frontier_to_primary() {
        let router = SmartRouter::new("gemma4:31b", "qwen3:4b");
        let decision = router
            .route("Perform a security audit and vulnerability scan of the production system")
            .await;
        assert_eq!(decision.provider, ProviderChoice::Primary);
    }

    #[tokio::test]
    async fn router_stats() {
        let router = SmartRouter::new("gemma4:31b", "qwen3:4b");
        router.route("Hello!").await; // Flash → cheap
        router
            .route("Perform a security audit of this system")
            .await; // Frontier → primary
        let stats = router.stats().await;
        assert_eq!(stats.total_requests, 2);
        assert!(stats.cheap_requests >= 1);
        assert!(stats.primary_requests >= 1);
    }
}
