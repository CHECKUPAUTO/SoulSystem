//! Typed coding-tool dispatch for the canonical runtime.

use crate::command::CommandSpec;
use crate::runner::CommandRunner;
use crate::workspace::WorkspaceContext;
use soul_llm::provider::ToolSchema;
use soul_sandbox::SandboxPolicy;
use soul_tools::{required_permission_for, PermissionLevel, ToolId};
use soullink_gate::{
    spotlight, ApprovalGate, ApprovalRequirement, ExecutionMode, GateDecision, RiskLevel, Verdict,
};
use std::sync::Arc;
use std::time::Duration;

const DEFAULT_COMMAND_TIMEOUT_SECS: u64 = 60;
const MAX_COMMAND_TIMEOUT_SECS: u64 = 600;

#[derive(Debug, Clone)]
pub struct ToolExecutionResult {
    pub output: String,
    pub success: bool,
    pub permission: PermissionLevel,
}

#[derive(Clone)]
pub struct CodingToolExecutor {
    runner: crate::runner::SandboxCommandRunner,
    gate: Arc<ApprovalGate>,
}

impl CodingToolExecutor {
    pub fn new(policy: SandboxPolicy, mode: ExecutionMode) -> Self {
        Self {
            runner: crate::runner::SandboxCommandRunner::new(policy),
            gate: Arc::new(ApprovalGate::new(mode)),
        }
    }

    pub fn with_gate(policy: SandboxPolicy, gate: Arc<ApprovalGate>) -> Self {
        Self {
            runner: crate::runner::SandboxCommandRunner::new(policy),
            gate,
        }
    }

    pub fn gate(&self) -> &ApprovalGate {
        self.gate.as_ref()
    }

    pub async fn execute(
        &self,
        tool_name: &str,
        arguments: serde_json::Value,
        workspace: &WorkspaceContext,
    ) -> ToolExecutionResult {
        let tool = match ToolId::from_name(tool_name) {
            Ok(tool) => tool,
            Err(error) => {
                return ToolExecutionResult {
                    output: error.to_string(),
                    success: false,
                    permission: PermissionLevel::Destructive,
                };
            }
        };

        let command = arguments.get("command").and_then(serde_json::Value::as_str);
        let permission = required_permission_for(tool_name, command);
        let risk = risk_for(permission);
        let requirement = ApprovalRequirement {
            risk,
            reason: format!("{tool_name} requires {permission:?} permission"),
            auto_approve_safe: risk == RiskLevel::Safe,
        };
        let scope = workspace.root().display().to_string();
        match self.gate.evaluate(tool_name, &scope, &requirement).await {
            GateDecision::Allow => {}
            GateDecision::Deny(reason) | GateDecision::Pause(reason) => {
                return ToolExecutionResult {
                    output: reason,
                    success: false,
                    permission,
                };
            }
        }

        let raw = match tool {
            ToolId::ExecuteShell => self.execute_shell(arguments, workspace).await,
            ToolId::ReadFile | ToolId::WriteFile | ToolId::PatchFile => {
                self.execute_file_tool(tool_name, arguments, workspace)
                    .await
            }
            ToolId::BrowserRead | ToolId::McpCall => {
                Err("network/browser tools are not enabled in the coding runtime".to_string())
            }
        };
        let (success, output) = match raw {
            Ok(output) => (true, output),
            Err(error) => (false, error),
        };

        let (decision, report) = self.gate.screen_tool_output(tool_name, &output);
        match decision {
            GateDecision::Deny(reason) => ToolExecutionResult {
                output: format!("{reason}\n[tool output quarantined]"),
                success: false,
                permission,
            },
            GateDecision::Pause(reason) => ToolExecutionResult {
                output: format!("{reason}\n[tool output requires review]"),
                success: false,
                permission,
            },
            GateDecision::Allow if report.verdict == Verdict::Clean => ToolExecutionResult {
                output,
                success,
                permission,
            },
            GateDecision::Allow => ToolExecutionResult {
                output: spotlight(&output),
                success,
                permission,
            },
        }
    }

    async fn execute_shell(
        &self,
        arguments: serde_json::Value,
        workspace: &WorkspaceContext,
    ) -> Result<String, String> {
        let command = arguments
            .get("command")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "execute_shell requires a command".to_string())?;
        let command = CommandSpec::parse(command).map_err(|error| error.to_string())?;
        let timeout_secs = arguments
            .get("timeout_secs")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(DEFAULT_COMMAND_TIMEOUT_SECS)
            .clamp(1, MAX_COMMAND_TIMEOUT_SECS);
        let runner = self.runner.clone();
        let working_dir = workspace.worktree().to_path_buf();
        let output = tokio::task::spawn_blocking(move || {
            runner.run(
                &command,
                &working_dir,
                Duration::from_secs(timeout_secs.max(1)),
            )
        })
        .await
        .map_err(|error| format!("tool worker failed: {error}"))?
        .map_err(|error| error.to_string())?;

        let text = merge_output(&output.stdout, &output.stderr);
        if output.exit_code == Some(0) && !output.timed_out {
            Ok(text)
        } else {
            Err(format!("command failed: {text}"))
        }
    }

    async fn execute_file_tool(
        &self,
        tool_name: &str,
        arguments: serde_json::Value,
        workspace: &WorkspaceContext,
    ) -> Result<String, String> {
        let root = workspace.worktree().to_path_buf();
        let name = tool_name.to_string();
        tokio::task::spawn_blocking(move || soul_tools::dispatch_tool_in(&name, arguments, &root))
            .await
            .map_err(|error| format!("file tool worker failed: {error}"))?
    }
}

pub fn coding_tool_schemas() -> Vec<ToolSchema> {
    vec![
        ToolSchema {
            name: "read_file".into(),
            description: "Read a UTF-8 file relative to the coding worktree.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {"path": {"type": "string"}},
                "required": ["path"],
                "additionalProperties": false
            }),
        },
        ToolSchema {
            name: "write_file".into(),
            description: "Create or replace a UTF-8 file in the coding worktree.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "content": {"type": "string"}
                },
                "required": ["path", "content"],
                "additionalProperties": false
            }),
        },
        ToolSchema {
            name: "patch_file".into(),
            description: "Replace one exact text occurrence in a worktree file.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "old_text": {"type": "string"},
                    "new_text": {"type": "string"}
                },
                "required": ["path", "old_text", "new_text"],
                "additionalProperties": false
            }),
        },
        ToolSchema {
            name: "execute_shell".into(),
            description: "Run one shell-free command in the coding worktree; pipes and shell control syntax are rejected.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {"type": "string"},
                    "timeout_secs": {"type": "integer", "minimum": 1}
                },
                "required": ["command"],
                "additionalProperties": false
            }),
        },
    ]
}

fn risk_for(permission: PermissionLevel) -> RiskLevel {
    match permission {
        PermissionLevel::Read => RiskLevel::Safe,
        PermissionLevel::Write => RiskLevel::Medium,
        PermissionLevel::Destructive => RiskLevel::Critical,
    }
}

fn merge_output(stdout: &str, stderr: &str) -> String {
    match (stdout.is_empty(), stderr.is_empty()) {
        (true, true) => "(no output)".into(),
        (false, true) => stdout.into(),
        (true, false) => stderr.into(),
        (false, false) => format!("{stdout}\n{stderr}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::WorkspaceContext;
    use soul_sandbox::SandboxPolicy;
    use soullink_gate::ExecutionMode;

    #[test]
    fn schemas_expose_only_registered_coding_tools() {
        let names: Vec<_> = coding_tool_schemas()
            .into_iter()
            .map(|schema| schema.name)
            .collect();
        assert_eq!(
            names,
            ["read_file", "write_file", "patch_file", "execute_shell"]
        );
    }

    #[test]
    fn destructive_permission_maps_to_critical_risk() {
        assert_eq!(risk_for(PermissionLevel::Destructive), RiskLevel::Critical);
    }

    #[tokio::test]
    async fn autonomous_mode_blocks_destructive_shell_before_spawn() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = WorkspaceContext::new(dir.path(), dir.path(), "base", "session").unwrap();
        let executor = CodingToolExecutor::new(SandboxPolicy::default(), ExecutionMode::Autonomous);
        let result = executor
            .execute(
                "execute_shell",
                serde_json::json!({"command": "rm -rf /tmp/soul-coding-test"}),
                &workspace,
            )
            .await;

        assert!(!result.success);
        assert_eq!(result.permission, PermissionLevel::Destructive);
        assert!(result.output.contains("Autonomous mode blocks"));
    }
}
