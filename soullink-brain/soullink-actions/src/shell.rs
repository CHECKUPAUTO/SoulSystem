//! ShellExecutor — secure async shell command execution.
//!
//! Implements AsyncTool with a deny-list for dangerous commands.

use crate::tool::{AsyncTool, ToolError, ToolResult};
use std::time::{Duration, Instant};

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

    /// Run a command.
    ///
    /// `Self::is_dangerous` is a string denylist, and it stays as a cheap first
    /// pass, but it was the *only* thing between this tool's input and `sh -c`.
    /// Execution now goes through [`soul_sandbox::Sandbox`] (INV-EXEC-1), which
    /// normalises encoding tricks before matching, refuses `sh` as a head
    /// binary, and applies seccomp, `setrlimit` and a network namespace that no
    /// amount of string matching can substitute for.
    ///
    /// **Pipelines and redirects no longer work here.** The sandbox neutralises
    /// them; a shell is exactly what it exists to avoid. That is a behaviour
    /// change for callers that passed `a | b`, and they now get an error rather
    /// than a silently different command.
    async fn execute(&self, input: &str) -> Result<ToolResult, ToolError> {
        self.validate(input)?;

        let start = Instant::now();
        let policy = soul_sandbox::SandboxPolicy {
            timeout: Duration::from_secs(self.timeout_secs),
            // Carried through rather than dropped: a command that used to run
            // in `workdir` and now runs wherever the host process happens to
            // be would not fail, it would quietly do the wrong thing.
            working_dir: self.workdir.clone().map(Into::into),
            ..Default::default()
        };
        let command = input.to_string();

        let verdict = tokio::task::spawn_blocking(move || {
            soul_sandbox::Sandbox::new(policy).execute(&command)
        })
        .await
        .map_err(|e| ToolError::ExecutionFailed(format!("join error: {e}")))?
        .map_err(|e| match e {
            // A refusal is a policy decision, not an execution failure, and
            // the two must not be reported the same way: a caller retrying an
            // "execution failed" makes sense, retrying a refusal does not.
            soul_sandbox::SandboxError::Io(io) => ToolError::ExecutionFailed(io.to_string()),
            other => ToolError::PermissionDenied(format!("sandbox refused command: {other}")),
        })?;

        // The sandbox enforces the deadline itself and reports it as a field,
        // so the outer `tokio::time::timeout` is gone. Mapping it back to
        // `ToolError::Timeout` keeps this tool's contract: callers that
        // matched on `Timeout` still get it, rather than a success carrying a
        // partial result and no exit code.
        if verdict.timed_out {
            return Err(ToolError::Timeout(self.timeout_secs));
        }

        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(ToolResult {
            tool: "shell".into(),
            exit_code: verdict.exit_code.unwrap_or(-1),
            stdout: verdict.stdout,
            stderr: verdict.stderr,
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
