//! Loop conditions for iterative workflow nodes.

use serde::Deserialize;
use std::process::Command;

/// Loop configuration attached to a node.
#[derive(Debug, Clone, Deserialize)]
pub struct LoopConfig {
    /// Prompt to use for each iteration (overrides node prompt).
    pub prompt: String,
    /// Condition to exit the loop.
    #[serde(default, rename = "until")]
    pub condition: LoopCondition,
    /// Maximum iterations before forcing exit.
    #[serde(default = "default_max_iterations")]
    pub max_iterations: usize,
    /// Whether to reset AI session context each iteration.
    #[serde(default)]
    pub fresh_context: bool,
    /// Whether this is an interactive approval loop.
    #[serde(default)]
    pub interactive: bool,
}

fn default_max_iterations() -> usize {
    10
}

/// Conditions that determine when a loop exits.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LoopCondition {
    /// All sub-tasks report completion.
    AllTasksComplete,
    /// A human has approved the output.
    Approved,
    /// A bash command exits 0 (e.g. test suite).
    TestsPass(String),
    /// Hard iteration cap — always included via max_iterations.
    MaxIterations(usize),
    /// Custom expression (reserved for future eval).
    Custom(String),
}

impl Default for LoopCondition {
    fn default() -> Self {
        LoopCondition::MaxIterations(10)
    }
}

impl LoopCondition {
    /// Evaluate the condition. Returns true when the loop should EXIT.
    pub fn is_satisfied(&self, iteration: usize) -> bool {
        match self {
            LoopCondition::MaxIterations(max) => iteration >= *max,
            LoopCondition::AllTasksComplete => {
                // Simplified: in real use, check context state.
                // For now, treat as always unsatisfied (relies on max_iterations).
                false
            }
            LoopCondition::Approved => {
                // Approval must come from external signal; unsatisfied by default.
                false
            }
            LoopCondition::TestsPass(cmd) => {
                // Run the bash command synchronously; exit 0 = pass.
                let status = Command::new("sh")
                    .arg("-c")
                    .arg(cmd)
                    .output()
                    .ok()
                    .and_then(|o| o.status.code());
                status == Some(0)
            }
            LoopCondition::Custom(_) => {
                // Reserved; not yet evaluated.
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn max_iterations_condition() {
        assert!(LoopCondition::MaxIterations(3).is_satisfied(3));
        assert!(LoopCondition::MaxIterations(3).is_satisfied(5));
        assert!(!LoopCondition::MaxIterations(3).is_satisfied(2));
    }

    #[test]
    fn all_tasks_never_satisfied() {
        assert!(!LoopCondition::AllTasksComplete.is_satisfied(0));
    }

    #[test]
    fn approved_never_satisfied_automatically() {
        assert!(!LoopCondition::Approved.is_satisfied(0));
    }

    #[test]
    fn tests_pass_false_command() {
        // "false" always exits 1
        assert!(!LoopCondition::TestsPass("false".into()).is_satisfied(0));
    }

    #[test]
    fn tests_pass_true_command() {
        assert!(LoopCondition::TestsPass("true".into()).is_satisfied(0));
    }

    #[test]
    fn deserialize_loop_config() {
        let yaml = r#"
prompt: "implement fix"
until: ALL_TASKS_COMPLETE
max_iterations: 5
fresh_context: true
"#;
        let config: LoopConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.condition, LoopCondition::AllTasksComplete);
        assert_eq!(config.max_iterations, 5);
        assert!(config.fresh_context);
    }
}
