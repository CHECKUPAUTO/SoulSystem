use crate::models::Plan;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct PolicyVerdict {
    pub allowed: bool,
    pub reason: Option<String>,
    pub risk_score: f32,
}

/// The PolicyEngine enforces security and operational safety rules.
pub struct PolicyEngine;

impl PolicyEngine {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Check a plan against configured policies.
    pub fn check_plan(&self, plan: &Plan) -> PolicyVerdict {
        let mut risk_score = 0.0;

        // Simple heuristic rules for policy enforcement
        for constraint in &plan.constraints {
            let lower = constraint.to_lowercase();
            if lower.contains("privileged") || lower.contains("root") || lower.contains("unsafe") {
                risk_score += 0.4;
            }
            if lower.contains("network") || lower.contains("internet") {
                risk_score += 0.2;
            }
        }

        if risk_score > 0.8 {
            PolicyVerdict {
                allowed: false,
                reason: Some(
                    "Task exceeds maximum risk threshold for autonomous execution".to_string(),
                ),
                risk_score,
            }
        } else {
            PolicyVerdict {
                allowed: true,
                reason: None,
                risk_score,
            }
        }
    }

    /// Validate a command before execution in the sandbox.
    pub fn validate_command(&self, cmd: &str) -> bool {
        let forbidden = ["rm -rf /", "mkfs", "dd if=", ":(){ :|:& };:"];
        for f in forbidden {
            if cmd.contains(f) {
                return false;
            }
        }
        true
    }
}

impl Default for PolicyEngine {
    fn default() -> Self {
        Self::new()
    }
}
