//! Command execution adapter for the coding harness.
//!
//! The harness depends on this small trait rather than constructing
//! `std::process::Command` itself. Production execution uses the existing
//! `soul_sandbox` policy; deterministic test doubles can implement the trait
//! without launching a process.

use crate::command::CommandSpec;
use soul_sandbox::{Sandbox, SandboxError, SandboxPolicy, SandboxVerdict};
use std::path::Path;
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    pub timed_out: bool,
}

impl From<SandboxVerdict> for CommandOutput {
    fn from(verdict: SandboxVerdict) -> Self {
        Self {
            stdout: verdict.stdout,
            stderr: verdict.stderr,
            exit_code: verdict.exit_code,
            duration_ms: verdict.duration_ms,
            timed_out: verdict.timed_out,
        }
    }
}

pub trait CommandRunner: Send + Sync {
    fn run(
        &self,
        command: &CommandSpec,
        working_dir: &Path,
        timeout: Duration,
    ) -> Result<CommandOutput, RunnerError>;
}

#[derive(Debug, Clone)]
pub struct SandboxCommandRunner {
    policy: SandboxPolicy,
}

impl SandboxCommandRunner {
    pub fn new(policy: SandboxPolicy) -> Self {
        Self { policy }
    }

    pub fn policy(&self) -> &SandboxPolicy {
        &self.policy
    }
}

impl Default for SandboxCommandRunner {
    fn default() -> Self {
        Self::new(SandboxPolicy::default())
    }
}

impl CommandRunner for SandboxCommandRunner {
    fn run(
        &self,
        command: &CommandSpec,
        working_dir: &Path,
        timeout: Duration,
    ) -> Result<CommandOutput, RunnerError> {
        if !working_dir.is_dir() {
            return Err(RunnerError::WorkingDirectory(
                working_dir.display().to_string(),
            ));
        }

        let mut policy = self.policy.clone();
        policy.working_dir = Some(working_dir.to_path_buf());
        policy.timeout = timeout;

        let sandbox = Sandbox::new(policy);
        let verdict = sandbox.execute_spec(&command.to_spawn_spec())?;
        Ok(verdict.into())
    }
}

#[derive(Debug, Error)]
pub enum RunnerError {
    #[error("sandbox command failed: {0}")]
    Sandbox(#[from] SandboxError),
    #[error("working directory is not a directory: {0}")]
    WorkingDirectory(String),
}
