//! Acceptance-check execution and evidence collection.

use crate::command::{CommandSpec, CommandSpecError};
use crate::contract::{CheckResult, TaskSpec};
use crate::runner::{CommandRunner, RunnerError};
use crate::workspace::WorkspaceContext;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationReport {
    pub checks: Vec<CheckResult>,
}

impl VerificationReport {
    pub fn required_passed(&self, task: &TaskSpec) -> bool {
        task.acceptance
            .iter()
            .filter(|check| check.required)
            .all(|required| {
                self.checks
                    .iter()
                    .find(|result| result.name == required.name)
                    .is_some_and(|result| result.required && result.passed)
            })
    }

    pub fn all_passed(&self) -> bool {
        self.checks.iter().all(|check| check.passed)
    }
}

pub struct Verifier<R> {
    runner: R,
}

impl<R> Verifier<R>
where
    R: CommandRunner,
{
    pub fn new(runner: R) -> Self {
        Self { runner }
    }

    pub fn verify(&self, task: &TaskSpec, workspace: &WorkspaceContext) -> VerificationReport {
        let checks = task
            .acceptance
            .iter()
            .map(|check| self.run_check(check, workspace))
            .collect();
        VerificationReport { checks }
    }

    fn run_check(
        &self,
        check: &crate::contract::CheckSpec,
        workspace: &WorkspaceContext,
    ) -> CheckResult {
        let command = match CommandSpec::parse(&check.command) {
            Ok(command) => command,
            Err(error) => {
                return CheckResult::failed(
                    check.name.clone(),
                    check.required,
                    None,
                    0,
                    format_command_error(error),
                );
            }
        };

        let output = match self.runner.run(
            &command,
            workspace.worktree(),
            Duration::from_secs(check.timeout_secs),
        ) {
            Ok(output) => output,
            Err(error) => {
                return CheckResult::failed(
                    check.name.clone(),
                    check.required,
                    None,
                    0,
                    format_runner_error(error),
                );
            }
        };

        let passed = output.exit_code == Some(0) && !output.timed_out;
        let evidence = format_output(&output.stdout, &output.stderr, output.timed_out);
        if passed {
            CheckResult::passed(
                check.name.clone(),
                check.required,
                output.exit_code,
                output.duration_ms,
                evidence,
            )
        } else {
            CheckResult::failed(
                check.name.clone(),
                check.required,
                output.exit_code,
                output.duration_ms,
                evidence,
            )
        }
    }
}

fn format_output(stdout: &str, stderr: &str, timed_out: bool) -> String {
    let mut output = String::new();
    if !stdout.is_empty() {
        output.push_str("stdout:\n");
        output.push_str(stdout);
    }
    if !stderr.is_empty() {
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str("stderr:\n");
        output.push_str(stderr);
    }
    if timed_out {
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str("[sandbox] command timed out");
    }
    if output.is_empty() {
        output.push_str("(no output)");
    }
    output
}

fn format_command_error(error: CommandSpecError) -> String {
    format!("check command rejected: {error}")
}

fn format_runner_error(error: RunnerError) -> String {
    format!("check command could not execute: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::CommandSpec;
    use crate::contract::{CheckSpec, TaskSpec};
    use crate::runner::{CommandOutput, RunnerError};
    use std::path::Path;

    #[derive(Clone)]
    struct FakeRunner {
        output: CommandOutput,
    }

    impl CommandRunner for FakeRunner {
        fn run(
            &self,
            _command: &CommandSpec,
            _working_dir: &Path,
            _timeout: Duration,
        ) -> Result<CommandOutput, RunnerError> {
            Ok(self.output.clone())
        }
    }

    #[test]
    fn required_check_passes_only_on_zero_exit_without_timeout() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = WorkspaceContext::new(dir.path(), dir.path(), "base", "session").unwrap();
        let check = CheckSpec::required("unit", "echo ok", 10).unwrap();
        let task = TaskSpec::new("run verification", vec![check]).unwrap();
        let report = Verifier::new(FakeRunner {
            output: CommandOutput {
                stdout: "ok".into(),
                stderr: String::new(),
                exit_code: Some(0),
                duration_ms: 3,
                timed_out: false,
            },
        })
        .verify(&task, &workspace);

        assert!(report.required_passed(&task));
        assert!(report.all_passed());
        assert!(report.checks[0].required);
    }

    #[test]
    fn shell_syntax_becomes_failed_evidence() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = WorkspaceContext::new(dir.path(), dir.path(), "base", "session").unwrap();
        let check = CheckSpec::required("unit", "echo ok && echo bad", 10).unwrap();
        let task = TaskSpec::new("run verification", vec![check]).unwrap();
        let report = Verifier::new(FakeRunner {
            output: CommandOutput {
                stdout: String::new(),
                stderr: String::new(),
                exit_code: Some(0),
                duration_ms: 0,
                timed_out: false,
            },
        })
        .verify(&task, &workspace);

        assert!(!report.required_passed(&task));
        assert!(!report.checks[0].passed);
        assert!(report.checks[0].output.contains("shell-free"));
    }
}
