//! ShellExecutor — secure async shell command execution.
//!
//! Implements AsyncTool with a deny-list for dangerous commands.

use crate::tool::{AsyncTool, ToolError, ToolResult};
use std::time::{Duration, Instant};
use tokio::process::Command;

/// Commands that are explicitly denied.
const DENY_PATTERNS: &[&str] = &[
    "rm -rf /",
    "mkfs",
    "dd if=",
    ":(){:|:&};:",
    "format",
    "del /s",
    "shutdown",
    "reboot",
    "init 0",
    "init 6",
];

/// Maximum command timeout in seconds.
const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// A secure shell executor with validation and timeout.
pub struct ShellExecutor {
    /// Maximum execution time in seconds.
    pub timeout_secs: u64,
    /// Working directory for commands.
    pub workdir: Option<String>,
}

impl ShellExecutor {
    pub fn new() -> Self {
        Self {
            timeout_secs: DEFAULT_TIMEOUT_SECS,
            workdir: None,
        }
    }

    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    pub fn with_workdir(mut self, dir: impl Into<String>) -> Self {
        self.workdir = Some(dir.into());
        self
    }

    /// Check if a command contains dangerous patterns.
    fn is_dangerous(cmd: &str) -> bool {
        let lower = cmd.to_lowercase();
        DENY_PATTERNS.iter().any(|p| lower.contains(p))
    }
}

impl Default for ShellExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl AsyncTool for ShellExecutor {
    fn name(&self) -> &str {
        "shell"
    }

    fn validate(&self, input: &str) -> Result<(), ToolError> {
        if input.trim().is_empty() {
            return Err(ToolError::ValidationFailed("empty command".into()));
        }
        if Self::is_dangerous(input) {
            return Err(ToolError::PermissionDenied(format!(
                "command contains dangerous pattern: {}",
                input
            )));
        }
        Ok(())
    }

    async fn execute(&self, input: &str) -> Result<ToolResult, ToolError> {
        self.validate(input)?;

        let start = Instant::now();
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(input);

        if let Some(ref dir) = self.workdir {
            cmd.current_dir(dir);
        }

        let output = tokio::time::timeout(Duration::from_secs(self.timeout_secs), cmd.output())
            .await
            .map_err(|_| ToolError::Timeout(self.timeout_secs))?
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(ToolResult {
            tool: "shell".into(),
            exit_code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            duration_ms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_accepts_safe_commands() {
        let exec = ShellExecutor::new();
        assert!(exec.validate("ls -la").is_ok());
        assert!(exec.validate("echo hello").is_ok());
        assert!(exec.validate("cargo test").is_ok());
    }

    #[test]
    fn validate_rejects_dangerous_commands() {
        let exec = ShellExecutor::new();
        assert!(exec.validate("rm -rf /").is_err());
        assert!(exec.validate("mkfs.ext4 /dev/sda1").is_err());
        assert!(exec.validate("shutdown -h now").is_err());
    }

    #[test]
    fn validate_rejects_empty() {
        let exec = ShellExecutor::new();
        assert!(exec.validate("").is_err());
        assert!(exec.validate("   ").is_err());
    }

    #[tokio::test]
    async fn execute_echo() {
        let exec = ShellExecutor::new();
        let result = exec.execute("echo hello_world").await.unwrap();
        assert!(result.is_success());
        assert!(result.stdout.contains("hello_world"));
    }

    #[tokio::test]
    async fn execute_timeout() {
        let exec = ShellExecutor::new().with_timeout(1);
        let result = exec.execute("sleep 10").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::Timeout(_) => {} // expected
            e => panic!("expected Timeout, got {:?}", e),
        }
    }
}
