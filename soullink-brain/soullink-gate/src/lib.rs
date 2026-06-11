//! Approval gate for tool execution.
//! Ported from IronClaw gate/approval.rs.
//!
//! Three execution modes:
//! - **Interactive** — Requires human approval for dangerous operations
//! - **Autonomous** — Auto-approves safe operations, blocks dangerous ones
//! - **Container** — Everything runs in sandbox, auto-approve all

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Execution mode for the gate.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum ExecutionMode {
    Interactive,
    Autonomous,
    Container,
}

/// Gate decision outcome.
#[derive(Debug, Clone)]
pub enum GateDecision {
    Allow,
    Deny(String),
    Pause(String), // Requires human approval
}

/// Risk level of a tool call.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum RiskLevel {
    Safe = 0,
    Low = 1,
    Medium = 2,
    High = 3,
    Critical = 4,
}

/// Approval requirement for a tool.
#[derive(Debug, Clone)]
pub struct ApprovalRequirement {
    pub risk: RiskLevel,
    pub reason: String,
    pub auto_approve_safe: bool,
}

impl ApprovalRequirement {
    pub fn safe() -> Self {
        Self {
            risk: RiskLevel::Safe,
            reason: String::new(),
            auto_approve_safe: true,
        }
    }
    pub fn risky(reason: &str) -> Self {
        Self {
            risk: RiskLevel::High,
            reason: reason.to_string(),
            auto_approve_safe: false,
        }
    }
    pub fn critical(reason: &str) -> Self {
        Self {
            risk: RiskLevel::Critical,
            reason: reason.to_string(),
            auto_approve_safe: false,
        }
    }
}

/// Persistent permission store.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PermissionStore {
    /// Always-allow (tool, scope) pairs.
    always_allow: HashSet<String>,
    /// Always-deny (tool, scope) pairs.
    always_deny: HashSet<String>,
}

impl PermissionStore {
    pub fn key(tool: &str, scope: &str) -> String {
        format!("{}:{}", tool, scope)
    }

    pub fn always_allow(&mut self, tool: &str, scope: &str) {
        self.always_allow.insert(Self::key(tool, scope));
        self.always_deny.remove(&Self::key(tool, scope));
    }

    pub fn always_deny(&mut self, tool: &str, scope: &str) {
        self.always_deny.insert(Self::key(tool, scope));
        self.always_allow.remove(&Self::key(tool, scope));
    }

    pub fn is_allowed(&self, tool: &str, scope: &str) -> bool {
        self.always_allow.contains(&Self::key(tool, scope))
    }

    pub fn is_denied(&self, tool: &str, scope: &str) -> bool {
        self.always_deny.contains(&Self::key(tool, scope))
    }
}

/// The approval gate.
pub struct ApprovalGate {
    mode: ExecutionMode,
    permissions: Arc<RwLock<PermissionStore>>,
    /// Max risk level for autonomous mode.
    autonomous_threshold: RiskLevel,
}

impl ApprovalGate {
    pub fn new(mode: ExecutionMode) -> Self {
        Self {
            mode,
            permissions: Arc::new(RwLock::new(PermissionStore::default())),
            autonomous_threshold: RiskLevel::Medium,
        }
    }

    pub fn with_threshold(mut self, threshold: RiskLevel) -> Self {
        self.autonomous_threshold = threshold;
        self
    }

    /// Evaluate whether a tool call is allowed.
    pub async fn evaluate(
        &self,
        tool: &str,
        scope: &str,
        req: &ApprovalRequirement,
    ) -> GateDecision {
        let perms = self.permissions.read().await;

        // Check persistent permissions first
        if perms.is_allowed(tool, scope) {
            return GateDecision::Allow;
        }
        if perms.is_denied(tool, scope) {
            return GateDecision::Deny("Permanently denied".into());
        }

        drop(perms);

        match self.mode {
            ExecutionMode::Interactive => {
                if req.risk >= RiskLevel::High {
                    GateDecision::Pause(req.reason.clone())
                } else {
                    GateDecision::Allow
                }
            }
            ExecutionMode::Autonomous => {
                if req.risk > self.autonomous_threshold {
                    GateDecision::Deny(format!("Autonomous mode blocks {}: {}", tool, req.reason))
                } else {
                    GateDecision::Allow
                }
            }
            ExecutionMode::Container => GateDecision::Allow,
        }
    }

    /// Grant "always allow" permission.
    pub async fn grant_permission(&self, tool: &str, scope: &str) {
        self.permissions.write().await.always_allow(tool, scope);
    }

    /// Grant "always deny" permission.
    pub async fn deny_permission(&self, tool: &str, scope: &str) {
        self.permissions.write().await.always_deny(tool, scope);
    }

    /// Save permissions to file (debug format for v0.1).
    pub async fn save(&self, path: impl AsRef<Path>) -> Result<(), GateError> {
        let perms = self.permissions.read().await;
        let data = format!("{:?}", perms);
        tokio::fs::write(path, &data).await?;
        Ok(())
    }

    /// Load permissions from file (TODO: proper serialization).
    pub async fn load(&self, _path: impl AsRef<Path>) -> Result<(), GateError> {
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GateError {
    #[error("Persistence error: {0}")]
    Persistence(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn interactive_allows_safe() {
        let gate = ApprovalGate::new(ExecutionMode::Interactive);
        let req = ApprovalRequirement::safe();
        match gate.evaluate("read", "file.txt", &req).await {
            GateDecision::Allow => {}
            other => panic!("expected Allow, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn interactive_pauses_on_risky() {
        let gate = ApprovalGate::new(ExecutionMode::Interactive);
        let req = ApprovalRequirement::risky("Deletes files");
        match gate.evaluate("shell", "rm -rf /", &req).await {
            GateDecision::Pause(_) => {}
            other => panic!("expected Pause, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn autonomous_blocks_critical() {
        let gate = ApprovalGate::new(ExecutionMode::Autonomous);
        let req = ApprovalRequirement::critical("Factory reset");
        match gate.evaluate("system", "reset", &req).await {
            GateDecision::Deny(_) => {}
            other => panic!("expected Deny, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn persistent_permission() {
        let gate = ApprovalGate::new(ExecutionMode::Interactive);
        gate.grant_permission("shell", "ls").await;
        let req = ApprovalRequirement::risky("Shell access");
        match gate.evaluate("shell", "ls", &req).await {
            GateDecision::Allow => {}
            other => panic!("expected Allow with persistent permission, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn container_allows_all() {
        let gate = ApprovalGate::new(ExecutionMode::Container);
        let req = ApprovalRequirement::critical("Nuclear launch");
        match gate.evaluate("nuke", "launch", &req).await {
            GateDecision::Allow => {}
            other => panic!("expected Allow in container mode, got {:?}", other),
        }
    }
}
