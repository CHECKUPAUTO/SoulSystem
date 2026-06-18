//! Adaptive Strategy Selector
//!
//! Dynamically selects the best reasoning strategy (ReAct, Tree-of-Thoughts,
//! or Plan+Execute) based on task complexity, domain, and run-time feedback.
//!
//! ## Strategy selection rules:
//! - **ReAct**: simple tasks (< 50 chars, 1-2 steps)
//! - **PlanThenExecute**: medium tasks (50-200 chars, explicit multi-step)
//! - **TreeOfThoughts**: complex tasks (> 200 chars, creative/debug/research)
//! - **Fallback**: auto-escalate after 5 failed ReAct turns

use serde::{Deserialize, Serialize};

/// Reasoning strategy type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StrategyType {
    /// React loop: observe → think → act → evaluate. Best for simple tasks.
    ReAct,
    /// Tree of Thoughts: explore multiple reasoning paths, prune low-scoring branches.
    TreeOfThoughts,
    /// Plan first, then execute each step with ReAct. Best for multi-step tasks.
    PlanThenExecute,
}

impl std::fmt::Display for StrategyType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReAct => write!(f, "ReAct"),
            Self::TreeOfThoughts => write!(f, "TreeOfThoughts"),
            Self::PlanThenExecute => write!(f, "PlanThenExecute"),
        }
    }
}

/// Domain classification of a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskDomain {
    /// System administration (shell commands, file ops).
    System,
    /// Code-related (development, debugging, refactoring).
    Code,
    /// Research/analysis (information gathering, summarization).
    Research,
    /// Creative (writing, design, ideation).
    Creative,
    /// General/unknown.
    General,
}

impl std::fmt::Display for TaskDomain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::System => write!(f, "system"),
            Self::Code => write!(f, "code"),
            Self::Research => write!(f, "research"),
            Self::Creative => write!(f, "creative"),
            Self::General => write!(f, "general"),
        }
    }
}

/// Analyzed complexity of a task, used to select strategy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskComplexity {
    /// Character count of the task description.
    pub char_count: usize,
    /// Estimated number of steps needed.
    pub estimated_steps: usize,
    /// Detected domain.
    pub domain: TaskDomain,
    /// Complexity score (0.0–1.0).
    pub complexity_score: f32,
    /// Whether the task requires exploration/creativity.
    pub requires_exploration: bool,
    /// Whether the task has explicit multi-step structure.
    pub is_multi_step: bool,
}

/// Configuration for the strategy selector.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyConfig {
    /// Maximum chars for ReAct (above this, consider plan/ToT).
    pub react_max_chars: usize,
    /// Max chars for PlanThenExecute (above this, use ToT).
    pub plan_max_chars: usize,
    /// Minimum estimated steps for PlanThenExecute.
    pub plan_min_steps: usize,
    /// Minimum estimated steps for TreeOfThoughts.
    pub tot_min_steps: usize,
    /// Minimum complexity score for ToT.
    pub tot_min_complexity: f32,
    /// Number of ReAct failures before auto-escalation.
    pub escalation_after_failures: usize,
    /// Max ToT depth.
    pub tot_max_depth: usize,
    /// Max ToT branches per node.
    pub tot_max_branches: usize,
    /// Top-K nodes to keep in ToT pruning.
    pub tot_top_k: usize,
}

impl Default for StrategyConfig {
    fn default() -> Self {
        Self {
            react_max_chars: 150,
            plan_max_chars: 500,
            plan_min_steps: 2,
            tot_min_steps: 4,
            tot_min_complexity: 0.6,
            escalation_after_failures: 5,
            tot_max_depth: 4,
            tot_max_branches: 3,
            tot_top_k: 3,
        }
    }
}

/// Result returned by each strategy after execution.
#[derive(Debug, Clone)]
pub struct StrategyResult {
    /// The selected strategy type.
    pub strategy: StrategyType,
    /// The final response content.
    pub result: String,
    /// Number of turns/iterations used.
    pub turns_used: usize,
    /// Whether the task completed successfully.
    pub success: bool,
}

/// Adaptive strategy selector.
///
/// Analyzes task complexity and selects the optimal reasoning strategy.
/// Tracks performance metrics to learn which strategies work best for
/// different task types over time.
#[derive(Debug, Clone)]
pub struct StrategySelector {
    pub config: StrategyConfig,
    /// History of strategy outcomes for learning.
    pub history: Vec<StrategyOutcome>,
    /// Success count per strategy.
    react_success: usize,
    react_total: usize,
    plan_success: usize,
    plan_total: usize,
    tot_success: usize,
    tot_total: usize,
}

/// Recorded outcome of a strategy execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyOutcome {
    pub strategy: StrategyType,
    pub task_preview: String,
    pub char_count: usize,
    pub domain: TaskDomain,
    pub estimated_steps: usize,
    pub success: bool,
    pub turns_used: usize,
}

impl StrategySelector {
    pub fn new(config: StrategyConfig) -> Self {
        Self {
            config,
            history: Vec::new(),
            react_success: 0,
            react_total: 0,
            plan_success: 0,
            plan_total: 0,
            tot_success: 0,
            tot_total: 0,
        }
    }

    // ── Task Complexity Analysis ──────────────────────────────────────

    /// Analyze a task to determine its complexity profile.
    pub fn analyze(&self, task: &str) -> TaskComplexity {
        let task_lower = task.to_lowercase();
        let char_count = task.len();

        // Count steps indicators
        let step_markers = [
            "first", "then", "after", "next", "finally",
            "step 1", "step 2", "step 3",
            "1.", "2.", "3.", "4.", "5.",
            "1)", "2)", "3)", "4)", "5)",
            "first,", "second,", "third,",
        ];
        let marker_count: usize = step_markers
            .iter()
            .map(|m| task_lower.matches(m).count())
            .sum();
        let numbered_item_count = task_lower
            .lines()
            .filter(|l| {
                let trimmed = l.trim();
                trimmed.starts_with(|c: char| c.is_ascii_digit())
                    && (trimmed.contains('.') || trimmed.contains(')'))
            })
            .count();

        let estimated_steps = if marker_count > 0 {
            (marker_count + numbered_item_count).max(1)
        } else {
            // Estimate based on char count and structure
            if char_count < 100 {
                1
            } else if char_count < 300 {
                2
            } else if char_count < 800 {
                3
            } else if char_count < 2000 {
                5
            } else {
                7
            }
        };

        // Domain detection
        let domain = self.detect_domain(&task_lower);

        // Requires exploration (creative, design, debug, research tasks)
        let exploration_keywords = [
            "design", "architect", "brainstorm", "ideate",
            "explore", "investigate", "debug", "trace", "diagnose",
            "optimize", "improve", "refactor", "restructure",
            "research", "analyze alternatives", "compare approaches",
            "evaluate tradeoffs", "novel", "innovative",
        ];
        let requires_exploration = exploration_keywords
            .iter()
            .any(|kw| task_lower.contains(kw));

        // Is explicitly multi-step?
        let is_multi_step = estimated_steps >= 2
            || task_lower.contains("steps:")
            || task_lower.contains("plan:")
            || task_lower.contains("instructions:");

        // Complexity score (0.0–1.0)
        let complexity_score = self.calculate_complexity(
            char_count,
            estimated_steps,
            requires_exploration,
            domain,
        );

        TaskComplexity {
            char_count,
            estimated_steps,
            domain,
            complexity_score,
            requires_exploration,
            is_multi_step,
        }
    }

    /// Detect the domain of a task from its content.
    fn detect_domain(&self, task: &str) -> TaskDomain {
        let code_keywords = [
            "code", "function", "class", "module", "crate", "cargo",
            "rustc", "compile", "borrow checker", "trait", "impl",
            "struct", "enum", "api", "endpoint", "http", "rest",
            "database", "sql", "query", "migrate", "test",
            "bug", "fix", "patch", "refactor",
            "npm", "pip", "docker", "kubernetes",
        ];
        let system_keywords = [
            "shell", "bash", "command", "directory", "file", "disk",
            "memory", "cpu", "process", "service", "daemon", "log",
            "permission", "chmod", "chown", "mount", "network",
            "port", "firewall", "cron",
        ];
        let research_keywords = [
            "research", "analyze", "summarize", "explain", "compare",
            "review", "audit", "evaluate", "assess", "study",
        ];
        let creative_keywords = [
            "write", "story", "poem", "creative", "brainstorm",
            "design", "draw", "visual", "layout",
        ];

        let code_count = code_keywords.iter().filter(|k| task.contains(*k)).count();
        let sys_count = system_keywords.iter().filter(|k| task.contains(*k)).count();
        let research_count = research_keywords.iter().filter(|k| task.contains(*k)).count();
        let creative_count = creative_keywords.iter().filter(|k| task.contains(*k)).count();

        let scores = [
            (code_count, TaskDomain::Code),
            (sys_count, TaskDomain::System),
            (research_count, TaskDomain::Research),
            (creative_count, TaskDomain::Creative),
        ];

        let (max_count, max_domain) = scores
            .iter()
            .max_by_key(|(c, _)| *c)
            .copied()
            .unwrap_or((0, TaskDomain::General));

        if max_count > 0 {
            max_domain
        } else {
            TaskDomain::General
        }
    }

    /// Calculate a 0.0–1.0 complexity score.
    fn calculate_complexity(
        &self,
        char_count: usize,
        estimated_steps: usize,
        requires_exploration: bool,
        _domain: TaskDomain,
    ) -> f32 {
        let length_score = (char_count as f32 / 2000.0).min(1.0);
        let step_score = (estimated_steps as f32 / 10.0).min(1.0);
        let exploration_bonus = if requires_exploration { 0.3 } else { 0.0 };

        // Weighted average
        (length_score * 0.3 + step_score * 0.5 + exploration_bonus).min(1.0)
    }

    // ── Strategy Selection ────────────────────────────────────────────

    /// Select the optimal strategy for a given task.
    pub fn select(&self, task: &str) -> StrategyType {
        let complexity = self.analyze(task);

        // Rule 0: Complex tasks requiring exploration → ToT (highest priority)
        if complexity.complexity_score >= self.config.tot_min_complexity
            || complexity.estimated_steps >= self.config.tot_min_steps
            || complexity.requires_exploration
        {
            return StrategyType::TreeOfThoughts;
        }

        // Rule 1: Multi-step tasks needing planning
        if complexity.estimated_steps >= self.config.plan_min_steps
            && complexity.is_multi_step
        {
            return StrategyType::PlanThenExecute;
        }

        // Rule 2: Very short, simple tasks → ReAct (fallback)
        StrategyType::ReAct
    }

    /// Select strategy considering previous failures (auto-escalation).
    pub fn select_with_failures(
        &self,
        task: &str,
        consecutive_failures: usize,
    ) -> StrategyType {
        // Escalation level 2: heavy failures → force ToT
        if consecutive_failures >= self.config.escalation_after_failures * 2 {
            return StrategyType::TreeOfThoughts;
        }

        // Escalation level 1: some failures → upgrade from ReAct to Plan
        if consecutive_failures >= self.config.escalation_after_failures {
            let base = self.select(task);
            if base == StrategyType::ReAct {
                return StrategyType::PlanThenExecute;
            }
            return base;
        }

        self.select(task)
    }

    // ── Performance Tracking ──────────────────────────────────────────

    /// Record the outcome of a strategy execution for learning.
    pub fn record_outcome(&mut self, outcome: StrategyOutcome) {
        match outcome.strategy {
            StrategyType::ReAct => {
                self.react_total += 1;
                if outcome.success {
                    self.react_success += 1;
                }
            }
            StrategyType::PlanThenExecute => {
                self.plan_total += 1;
                if outcome.success {
                    self.plan_success += 1;
                }
            }
            StrategyType::TreeOfThoughts => {
                self.tot_total += 1;
                if outcome.success {
                    self.tot_success += 1;
                }
            }
        }
        self.history.push(outcome);
        // Keep history bounded
        if self.history.len() > 200 {
            self.history.remove(0);
        }
    }

    /// Success rate for a given strategy.
    pub fn success_rate(&self, strategy: StrategyType) -> f32 {
        match strategy {
            StrategyType::ReAct => {
                if self.react_total == 0 {
                    0.0
                } else {
                    self.react_success as f32 / self.react_total as f32
                }
            }
            StrategyType::PlanThenExecute => {
                if self.plan_total == 0 {
                    0.0
                } else {
                    self.plan_success as f32 / self.plan_total as f32
                }
            }
            StrategyType::TreeOfThoughts => {
                if self.tot_total == 0 {
                    0.0
                } else {
                    self.tot_success as f32 / self.tot_total as f32
                }
            }
        }
    }

    /// Get a summary of strategy performance.
    pub fn performance_summary(&self) -> serde_json::Value {
        serde_json::json!({
            "ReAct": {
                "success": self.react_success,
                "total": self.react_total,
                "rate": self.success_rate(StrategyType::ReAct),
            },
            "PlanThenExecute": {
                "success": self.plan_success,
                "total": self.plan_total,
                "rate": self.success_rate(StrategyType::PlanThenExecute),
            },
            "TreeOfThoughts": {
                "success": self.tot_success,
                "total": self.tot_total,
                "rate": self.success_rate(StrategyType::TreeOfThoughts),
            },
            "history_size": self.history.len(),
        })
    }
}

impl Default for StrategySelector {
    fn default() -> Self {
        Self::new(StrategyConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_selector() -> StrategySelector {
        StrategySelector::default()
    }

    #[test]
    fn analyze_simple_task() {
        let sel = make_selector();
        let c = sel.analyze("What time is it?");
        assert_eq!(c.estimated_steps, 1);
        assert!(c.complexity_score < 0.3);
        assert!(!c.is_multi_step);
        assert!(!c.requires_exploration);
    }

    #[test]
    fn analyze_multi_step_task() {
        let sel = make_selector();
        let task = "First, read the file. Then, parse the JSON. After that, validate it. Finally, save the result.";
        let c = sel.analyze(task);
        assert!(c.estimated_steps >= 4, "got {}", c.estimated_steps);
        assert!(c.is_multi_step);
    }

    #[test]
    fn analyze_exploration_task() {
        let sel = make_selector();
        let c = sel.analyze("Design a new architecture for the microservices");
        assert!(c.requires_exploration);
    }

    #[test]
    fn select_react_for_simple() {
        let sel = make_selector();
        assert_eq!(sel.select("What time is it?"), StrategyType::ReAct);
        assert_eq!(sel.select("echo hello"), StrategyType::ReAct);
    }

    #[test]
    fn select_plan_for_multi_step() {
        let sel = make_selector();
        let task = "Step 1: Create a new Rust project. Step 2: Add dependencies. Step 3: Write the main function.";
        assert_eq!(sel.select(task), StrategyType::PlanThenExecute);
    }

    #[test]
    fn select_tot_for_complex() {
        let sel = make_selector();
        let task = "Design a distributed caching system that handles 1M requests/second with sub-ms latency. Consider cache coherency, eviction policies, and fault tolerance. Write a full implementation plan.";
        assert_eq!(sel.select(task), StrategyType::TreeOfThoughts);
    }

    #[test]
    fn escalation_after_failures() {
        let sel = make_selector();
        let task = "echo hello";
        // Normally ReAct
        assert_eq!(
            sel.select_with_failures(task, 0),
            StrategyType::ReAct
        );
        // After 5 failures, escalate to plan
        assert_eq!(
            sel.select_with_failures(task, 5),
            StrategyType::PlanThenExecute
        );
        // After 10 failures, escalate to ToT
        assert_eq!(
            sel.select_with_failures(task, 10),
            StrategyType::TreeOfThoughts
        );
    }

    #[test]
    fn domain_detection_code() {
        let sel = make_selector();
        let c = sel.analyze("Fix the bug in the rust function module");
        assert_eq!(c.domain, TaskDomain::Code);
    }

    #[test]
    fn domain_detection_system() {
        let sel = make_selector();
        let c = sel.analyze("Check disk usage and clean up log files");
        assert_eq!(c.domain, TaskDomain::System);
    }

    #[test]
    fn domain_detection_research() {
        let sel = make_selector();
        let c = sel.analyze("Research and analyze the best database for our use case");
        assert_eq!(c.domain, TaskDomain::Research);
    }

    #[test]
    fn record_and_track_performance() {
        let mut sel = make_selector();
        sel.record_outcome(StrategyOutcome {
            strategy: StrategyType::ReAct,
            task_preview: "test".into(),
            char_count: 10,
            domain: TaskDomain::General,
            estimated_steps: 1,
            success: true,
            turns_used: 1,
        });
        sel.record_outcome(StrategyOutcome {
            strategy: StrategyType::ReAct,
            task_preview: "test".into(),
            char_count: 10,
            domain: TaskDomain::General,
            estimated_steps: 1,
            success: false,
            turns_used: 1,
        });
        assert_eq!(sel.success_rate(StrategyType::ReAct), 0.5);
        assert_eq!(sel.react_total, 2);
        assert_eq!(sel.react_success, 1);
    }

    #[test]
    fn performance_summary_json() {
        let mut sel = make_selector();
        sel.record_outcome(StrategyOutcome {
            strategy: StrategyType::ReAct,
            task_preview: "test".into(),
            char_count: 10,
            domain: TaskDomain::General,
            estimated_steps: 1,
            success: true,
            turns_used: 1,
        });
        let summary = sel.performance_summary();
        let react = &summary["ReAct"];
        assert_eq!(react["total"], 1);
        assert_eq!(react["success"], 1);
    }

    #[test]
    fn config_defaults() {
        let cfg = StrategyConfig::default();
        assert_eq!(cfg.react_max_chars, 150);
        assert_eq!(cfg.plan_max_chars, 500);
        assert_eq!(cfg.plan_min_steps, 2);
        assert_eq!(cfg.escalation_after_failures, 5);
    }

    #[test]
    fn strategy_display() {
        assert_eq!(format!("{}", StrategyType::ReAct), "ReAct");
        assert_eq!(format!("{}", StrategyType::TreeOfThoughts), "TreeOfThoughts");
        assert_eq!(format!("{}", StrategyType::PlanThenExecute), "PlanThenExecute");
    }
}
