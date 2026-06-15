use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct AgentDef {
    pub name: String,
    pub description: String,
    pub tools: Vec<String>,
    pub model: String,
    pub effort: Option<String>,
    pub color: Option<String>,
    pub max_turns: Option<usize>,
    pub allowed_tools: Option<Vec<String>>,
    pub body: String,
    pub path: PathBuf,
    pub raw_frontmatter: String,
}

#[derive(Debug, Clone)]
pub struct AgentAudit {
    pub agent: AgentDef,
    pub has_valid_frontmatter: bool,
    pub has_name: bool,
    pub has_description: bool,
    pub description_quality: DescriptionQuality,
    pub has_tools: bool,
    pub has_model: bool,
    pub body_length: usize,
    pub issues: Vec<Issue>,
    pub quality_score: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DescriptionQuality {
    Missing,
    TooShort,
    Adequate,
    Good,
    Excellent,
}

impl DescriptionQuality {
    pub fn from_len(len: usize) -> Self {
        match len {
            0 => DescriptionQuality::Missing,
            1..=19 => DescriptionQuality::TooShort,
            20..=99 => DescriptionQuality::Adequate,
            100..=199 => DescriptionQuality::Good,
            _ => DescriptionQuality::Excellent,
        }
    }

    pub fn score(&self) -> f64 {
        match self {
            DescriptionQuality::Missing => 0.0,
            DescriptionQuality::TooShort => 0.25,
            DescriptionQuality::Adequate => 0.5,
            DescriptionQuality::Good => 0.75,
            DescriptionQuality::Excellent => 1.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Issue {
    pub severity: Severity,
    pub category: IssueCategory,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum Severity {
    Critical,
    High,
    Medium,
    Low,
    Info,
}

impl Severity {
    pub fn score(&self) -> f64 {
        match self {
            Severity::Critical => 1.0,
            Severity::High => 0.75,
            Severity::Medium => 0.5,
            Severity::Low => 0.25,
            Severity::Info => 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum IssueCategory {
    MissingField,
    InvalidFrontmatter,
    PoorDescription,
    EmptyBody,
    MissingTools,
    UnknownModel,
    NamingConvention,
    DuplicateAgent,
    MissingLanguageCoverage,
    Other,
}

#[derive(Debug, Clone)]
pub struct AuditReport {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub agent_count: usize,
    pub command_count: usize,
    pub skill_count: usize,
    pub rule_count: usize,
    pub audits: Vec<AgentAudit>,
    pub ecosystem_health: f64,
    pub language_coverage: HashMap<String, usize>,
    pub model_distribution: HashMap<String, usize>,
    pub tool_distribution: HashMap<String, usize>,
    pub scan_errors: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Gap {
    pub category: GapCategory,
    pub severity: Severity,
    pub title: String,
    pub description: String,
    pub suggestion: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum GapCategory {
    MissingLanguageAgent,
    MissingCommand,
    MissingSkill,
    MissingRule,
    IncompleteAgent,
    UnderrepresentedModel,
    NoDedicatedAgent,
    DuplicateCoverage,
    LowQualityAgent,
    Bottleneck,
}

#[derive(Debug, Clone)]
pub struct Bottleneck {
    pub area: String,
    pub impact: String,
    pub suggestion: String,
    pub severity: Severity,
}

#[derive(Debug, Clone)]
pub struct AnalysisResult {
    pub gaps: Vec<Gap>,
    pub bottlenecks: Vec<Bottleneck>,
    pub recommendations: Vec<String>,
    pub quality_histogram: Vec<usize>,
}

#[derive(Debug, Clone)]
pub struct ImprovementProposal {
    pub id: String,
    pub title: String,
    pub description: String,
    pub priority: u32,
    pub effort: Effort,
    pub impact: Impact,
    pub gap: GapCategory,
    pub template_type: TemplateType,
    pub target_name: String,
    pub target_path: Option<PathBuf>,
    pub template_content: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Effort {
    Trivial,
    Small,
    Medium,
    Large,
    XLarge,
}

impl Effort {
    pub fn as_u32(&self) -> u32 {
        match self {
            Effort::Trivial => 1,
            Effort::Small => 2,
            Effort::Medium => 3,
            Effort::Large => 4,
            Effort::XLarge => 5,
        }
    }

    pub fn label(&self) -> &str {
        match self {
            Effort::Trivial => "trivial",
            Effort::Small => "small",
            Effort::Medium => "medium",
            Effort::Large => "large",
            Effort::XLarge => "xlarge",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Impact {
    Minimal,
    Small,
    Moderate,
    High,
    Transformative,
}

impl Impact {
    pub fn as_u32(&self) -> u32 {
        match self {
            Impact::Minimal => 1,
            Impact::Small => 2,
            Impact::Moderate => 3,
            Impact::High => 4,
            Impact::Transformative => 5,
        }
    }

    pub fn label(&self) -> &str {
        match self {
            Impact::Minimal => "minimal",
            Impact::Small => "small",
            Impact::Moderate => "moderate",
            Impact::High => "high",
            Impact::Transformative => "transformative",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TemplateType {
    LanguageReviewer,
    LanguageBuildResolver,
    Command,
    Skill,
    Rule,
    ImprovedVersion,
}

#[derive(Debug, Clone)]
pub struct ImprovementResult {
    pub proposal_id: String,
    pub success: bool,
    pub path: Option<PathBuf>,
    pub message: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct IterationRecord {
    pub iteration: u64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub audit: AuditReport,
    pub analysis: AnalysisResult,
    pub improvements: Vec<ImprovementResult>,
    pub health_delta: f64,
    pub duration_ms: u64,
    pub meta_inference: String,
}

#[derive(Debug, Clone)]
pub struct EvolutionConfig {
    pub agents_dir: PathBuf,
    pub commands_dir: PathBuf,
    pub skills_dir: PathBuf,
    pub rules_dir: PathBuf,
    pub output_dir: PathBuf,
    pub auto_apply: bool,
    pub quality_threshold: f64,
    pub max_iterations: Option<u64>,
    pub verbose: bool,
}

impl EvolutionConfig {
    pub fn agents_root() -> PathBuf {
        PathBuf::from("/root/.agents")
    }
}

impl Default for EvolutionConfig {
    fn default() -> Self {
        let root = Self::agents_root();
        Self {
            agents_dir: root.join("agents"),
            commands_dir: root.join("commands"),
            skills_dir: root.join("skills"),
            rules_dir: root.join("rules"),
            output_dir: root.join("evolved"),
            auto_apply: false,
            quality_threshold: 0.5,
            max_iterations: None,
            verbose: false,
        }
    }
}

// ──────────────────────────────────────────
// Paper-Inspired Types: The Seven Pillars
// ──────────────────────────────────────────

// 1. GÖDEL MACHINE (Schmidhuber 2007) — Self-Referential Meta-Modification
// =========================================================================

/// A self-referential strategy that can modify itself, inspired by the Gödel Machine's
/// proof searcher. Each strategy includes its own modification rules and can propose
/// changes to any part of the system, including other strategies.
#[derive(Debug, Clone)]
pub struct GodelStrategy {
    pub name: String,
    pub description: String,
    pub code: String,
    pub version: u64,
    pub utility_history: Vec<f64>,
    pub parent: Option<String>,
    pub children: Vec<String>,
    pub mutation_rate: f64,
    pub last_modified: chrono::DateTime<chrono::Utc>,
    pub effectiveness: f64,
}

/// A proposed self-modification (Gödel Machine "proof" that a rewrite is beneficial)
#[derive(Debug, Clone)]
pub struct SelfModProposal {
    pub strategy_name: String,
    pub target_path: Vec<String>,
    pub new_code: String,
    pub expected_utility_gain: f64,
    pub proof: String,
    pub applied: bool,
    pub actual_utility_gain: Option<f64>,
}

// 2. DARWIN GÖDEL MACHINE (Zhang et al. 2025) — Archive-Based Open-Ended Evolution
// ===============================================================================

/// An entry in the agent archive. The DGM maintains a growing collection of diverse
/// agents, forming a tree of innovation rather than a single optimization trajectory.
#[derive(Debug, Clone)]
pub struct ArchiveEntry {
    pub id: String,
    pub agent_def: AgentDef,
    pub quality_score: f64,
    pub novelty_score: f64,
    pub parent_id: Option<String>,
    pub generation: u64,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub tags: Vec<String>,
}

/// The open-ended archive. Inspired by the Darwin Gödel Machine's approach of
/// maintaining a growing tree of diverse, high-quality agents.
#[derive(Debug, Clone)]
pub struct AgentArchive {
    pub entries: Vec<ArchiveEntry>,
    pub max_size: usize,
    pub novelty_weight: f64,
    pub quality_weight: f64,
    pub diversity_threshold: f64,
}

// 3. STOP (Zelikman et al. 2023) — Self-Taught Optimizer Improver
// =================================================================

/// An "improver" that improves other programs (and can improve itself).
/// Inspired by STOP: the improver generates new code that outperforms the original,
/// and the improved improver can then improve itself further.
#[derive(Debug, Clone)]
pub struct Improver {
    pub name: String,
    pub version: u64,
    pub strategy_type: ImproverStrategy,
    pub performance_history: Vec<f64>,
    pub best_performance: f64,
    pub mutation_method: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ImproverStrategy {
    BeamSearch,
    GeneticAlgorithm,
    SimulatedAnnealing,
    ReflexiveMutation,
    CrossoverOnly,
    RandomMutation,
}

// 4. REFLEXION (Shinn et al. 2023) — Verbal Reinforcement Learning
// ==================================================================

/// A self-reflection episode. After each attempt, the agent reflects on what worked
/// and what didn't, storing the reflection for future iterations.
#[derive(Debug, Clone)]
pub struct ReflexionEpisode {
    pub iteration: u64,
    pub action: String,
    pub outcome: String,
    pub reflection: String,
    pub lessons: Vec<String>,
    pub emotional_state: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

// 5. OPRO (Yang et al. 2023) — Optimization Trajectory
// =================================================================

/// The full trajectory of an optimization process. OPRO showed that LLMs optimize
/// better when they see the complete history of previous attempts and their scores.
#[derive(Debug, Clone)]
pub struct OptimizationTrajectory {
    pub target_name: String,
    pub attempts: Vec<TrajectoryPoint>,
    pub best_score: f64,
    pub best_attempt: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct TrajectoryPoint {
    pub iteration: usize,
    pub description: String,
    pub score: f64,
    pub change_description: String,
    pub strategy_used: String,
}

// 6. ALPHAZERO (Silver et al. 2017) — Self-Play Competition
// =================================================================

/// A self-play match between two agents (current best vs challenger).
/// Inspired by AlphaZero: the agent learns by playing against itself.
#[derive(Debug, Clone)]
pub struct SelfPlayMatch {
    pub agent_a: String,
    pub agent_b: String,
    pub agent_a_score: f64,
    pub agent_b_score: f64,
    pub winner: Option<String>,
    pub rounds: u64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

// 7. I.J. GOOD (1965) — Intelligence Explosion Detection
// =================================================================

/// Tracks the rate of improvement to detect compounding acceleration
/// (the "intelligence explosion" or "singularity" phenomenon).
#[derive(Debug, Clone)]
pub struct ExplosionMetrics {
    pub health_history: Vec<f64>,
    pub delta_history: Vec<f64>,
    pub first_derivative: Vec<f64>,
    pub second_derivative: Vec<f64>,
    pub acceleration_threshold: f64,
    pub is_exploding: bool,
    pub explosion_probability: f64,
    pub trend: String,
}

// ──────────────────────────────────────────
// Utility Function (Gödel Machine proof check)
// ──────────────────────────────────────────

/// A utility function that evaluates whether a proposed change is beneficial.
/// The Gödel Machine requires PROOF that a rewrite improves utility.
#[derive(Debug, Clone)]
pub struct UtilityFunction {
    pub health_weight: f64,
    pub coverage_weight: f64,
    pub quality_weight: f64,
    pub diversity_weight: f64,
    pub cost_weight: f64,
    pub improvement_rate_weight: f64,
}

impl Default for UtilityFunction {
    fn default() -> Self {
        Self {
            health_weight: 0.30,
            coverage_weight: 0.20,
            quality_weight: 0.25,
            diversity_weight: 0.10,
            cost_weight: 0.10,
            improvement_rate_weight: 0.05,
        }
    }
}

impl UtilityFunction {
    /// Compute the overall utility of the current ecosystem state
    pub fn evaluate(&self, report: &AuditReport, archive: &AgentArchive) -> f64 {
        let health_score = report.ecosystem_health;
        let coverage_score = if KNOWN_LANGUAGES.is_empty() {
            0.0
        } else {
            report.language_coverage.len() as f64 / KNOWN_LANGUAGES.len() as f64
        };
        let quality_score = if report.audits.is_empty() {
            0.0
        } else {
            report.audits.iter().map(|a| a.quality_score).sum::<f64>() / report.audits.len() as f64
        };
        let diversity_score = if archive.max_size > 0 {
            (archive.entries.len() as f64 / archive.max_size as f64).min(1.0)
        } else {
            0.0
        };

        self.health_weight * health_score
            + self.coverage_weight * coverage_score
            + self.quality_weight * quality_score
            + self.diversity_weight * diversity_score
    }

    /// Check if a proposed change is provably beneficial (Gödel Machine requirement)
    pub fn is_provably_beneficial(
        &self,
        current_utility: f64,
        projected_utility: f64,
        threshold: f64,
    ) -> (bool, String) {
        let gain = projected_utility - current_utility;
        if gain > threshold {
            (
                true,
                format!(
                    "Utility gain {:.4} exceeds threshold {:.4}: PROVEN BENEFICIAL",
                    gain, threshold
                ),
            )
        } else if gain > 0.0 {
            (
                true,
                format!(
                    "Utility gain {:.4} is positive but below threshold: MARGINAL",
                    gain
                ),
            )
        } else {
            (
                false,
                format!("Utility change {:.4} is non-positive: REJECTED", gain),
            )
        }
    }
}

// ──────────────────────────────────────────
// Explosion Detection (I.J. Good 1965)
// ──────────────────────────────────────────

impl ExplosionMetrics {
    pub fn new(acceleration_threshold: f64) -> Self {
        Self {
            health_history: Vec::new(),
            delta_history: Vec::new(),
            first_derivative: Vec::new(),
            second_derivative: Vec::new(),
            acceleration_threshold,
            is_exploding: false,
            explosion_probability: 0.0,
            trend: "stable".to_string(),
        }
    }

    /// Add a new health measurement and update all derivatives
    pub fn record(&mut self, health: f64) {
        if let Some(&prev) = self.health_history.last() {
            let delta = health - prev;
            self.delta_history.push(delta);
            self.health_history.push(health);

            // Compute first derivative (rate of improvement)
            if self.delta_history.len() >= 2 {
                let recent_deltas: Vec<f64> =
                    self.delta_history.iter().rev().take(3).copied().collect();
                let fd = recent_deltas.iter().sum::<f64>() / recent_deltas.len() as f64;
                self.first_derivative.push(fd);

                // Compute second derivative (acceleration)
                if self.first_derivative.len() >= 2 {
                    let sd = self.first_derivative[self.first_derivative.len() - 1]
                        - self.first_derivative[self.first_derivative.len() - 2];
                    self.second_derivative.push(sd);

                    // Check for intelligence explosion
                    if sd > self.acceleration_threshold && fd > 0.0 {
                        self.is_exploding = true;
                        self.explosion_probability = (self.explosion_probability + 0.1).min(1.0);
                        self.trend = "ACCELERATING".to_string();
                    } else if fd > 0.0 {
                        self.trend = "improving".to_string();
                    } else if fd < 0.0 {
                        self.trend = "declining".to_string();
                    } else {
                        self.trend = "stable".to_string();
                    }
                }
            } else {
                self.first_derivative.push(delta);
            }
        } else {
            self.health_history.push(health);
        }
    }
}

pub const KNOWN_LANGUAGES: &[&str] = &[
    "python",
    "rust",
    "typescript",
    "javascript",
    "go",
    "java",
    "kotlin",
    "cpp",
    "csharp",
    "swift",
    "fsharp",
    "dart",
    "php",
    "ruby",
    "scala",
    "elixir",
    "clojure",
    "haskell",
    "lua",
    "r",
    "matlab",
    "julia",
    "zig",
    "nim",
    "crystal",
    "ocaml",
    "erlang",
    "perl",
    "django",
    "fastapi",
    "react",
    "flutter",
    "vue",
    "angular",
    "svelte",
    "nextjs",
    "nuxt",
    "pytorch",
    "tensorflow",
    "jax",
    "spring",
    "dotnet",
    "rails",
];

pub const KNOWN_MODELS: &[&str] = &["opus", "sonnet", "haiku"];

pub const KNOWN_TOOLS: &[&str] = &[
    "Read",
    "Write",
    "Edit",
    "Grep",
    "Glob",
    "Bash",
    "WebSearch",
    "WebFetch",
    "Skill",
    "Task",
    "Question",
    "Todowrite",
];
