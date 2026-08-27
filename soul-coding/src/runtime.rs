//! Canonical verification finalization for a coding session.
//!
//! The model loop will call this boundary after it has made edits. It is the
//! only place that can turn a worktree plus check evidence into a completed
//! `TaskResult`.

use crate::contract::{CompletionError, TaskResult, TaskSpec};
use crate::git::{GitError, GitWorkspace};
use crate::runner::CommandRunner;
use crate::verify::Verifier;

pub struct CodingRuntime<R> {
    runner: R,
}

impl<R> CodingRuntime<R>
where
    R: CommandRunner + Clone,
{
    pub fn new(runner: R) -> Self {
        Self { runner }
    }

    /// Verify the current worktree and produce an evidence-bearing result.
    ///
    /// A model message cannot complete a task. Completion requires the Git
    /// change set and every required acceptance check to pass; otherwise the
    /// returned result remains `Inconclusive` and carries the observed checks
    /// for a later resume.
    pub fn verify_workspace(
        &self,
        task: &TaskSpec,
        workspace: &GitWorkspace<R>,
    ) -> Result<TaskResult, RuntimeError> {
        let changes = workspace.change_set()?;
        let report = Verifier::new(self.runner.clone()).verify(task, workspace.context());
        let session_id = Some(workspace.context().session_id().to_string());

        if report.required_passed(task) {
            match TaskResult::completed(
                task,
                "Required acceptance checks passed.",
                changes.clone(),
                report.checks.clone(),
                session_id.clone(),
            ) {
                Ok(result) => Ok(result),
                Err(CompletionError::NoChanges) => Ok(TaskResult::inconclusive_with_evidence(
                    task.id.clone(),
                    "Verification passed, but the worktree contains no changes.",
                    "a completed task must leave an auditable change set",
                    changes,
                    report.checks,
                    session_id,
                )),
                Err(error) => Err(RuntimeError::Completion(error)),
            }
        } else {
            Ok(TaskResult::inconclusive_with_evidence(
                task.id.clone(),
                "At least one required acceptance check did not pass.",
                "required acceptance evidence is incomplete",
                changes,
                report.checks,
                session_id,
            ))
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("could not collect Git workspace evidence: {0}")]
    Git(#[from] GitError),
    #[error("could not finalize task result: {0}")]
    Completion(#[from] CompletionError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::CommandSpec;
    use crate::contract::{CheckSpec, TaskStatus};
    use crate::runner::{CommandOutput, RunnerError};
    use std::path::Path;
    use std::sync::Arc;
    use std::time::Duration;

    #[derive(Clone)]
    struct FakeRunner {
        output: Arc<CommandOutput>,
    }

    impl CommandRunner for FakeRunner {
        fn run(
            &self,
            command: &CommandSpec,
            _working_dir: &Path,
            _timeout: Duration,
        ) -> Result<CommandOutput, RunnerError> {
            if command.program() == "git" && command.display().contains("status") {
                return Ok(CommandOutput {
                    stdout: "?? src/lib.rs\0".into(),
                    exit_code: Some(0),
                    ..(*self.output).clone()
                });
            }
            if command.program() == "git" && command.display().contains("diff") {
                return Ok(CommandOutput {
                    stdout: "diff --git a/src/lib.rs b/src/lib.rs\n".into(),
                    exit_code: Some(0),
                    ..(*self.output).clone()
                });
            }
            Ok((*self.output).clone())
        }
    }

    #[test]
    fn failed_verification_is_not_reported_as_completed() {
        let dir = tempfile::tempdir().unwrap();
        let context =
            crate::workspace::WorkspaceContext::new(dir.path(), dir.path(), "base", "session")
                .unwrap();
        let runner = FakeRunner {
            output: Arc::new(CommandOutput {
                stdout: String::new(),
                stderr: "failure".into(),
                exit_code: Some(1),
                duration_ms: 2,
                timed_out: false,
            }),
        };
        let workspace = GitWorkspace::from_context(context, runner.clone());
        let task = TaskSpec::new(
            "implement change",
            vec![CheckSpec::required("unit", "cargo test", 30).unwrap()],
        )
        .unwrap();
        let result = CodingRuntime::new(runner)
            .verify_workspace(&task, &workspace)
            .unwrap();

        assert_eq!(result.status, TaskStatus::Inconclusive);
        assert_eq!(result.changes.unwrap().files.len(), 1);
        assert!(!result.checks[0].passed);
    }
}
