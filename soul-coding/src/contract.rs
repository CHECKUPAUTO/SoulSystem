//! Task, verification, and change-set contracts.
//!
//! A model response is not evidence of completion. The only constructor for a
//! completed result requires a non-empty change set and successful required
//! checks declared by the task.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Component, Path};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskSpec {
    pub id: String,
    pub prompt: String,
    pub acceptance: Vec<CheckSpec>,
    pub created_at: DateTime<Utc>,
}

impl TaskSpec {
    pub fn new(
        prompt: impl Into<String>,
        acceptance: Vec<CheckSpec>,
    ) -> Result<Self, ContractError> {
        let prompt = prompt.into();
        if prompt.trim().is_empty() {
            return Err(ContractError::EmptyPrompt);
        }
        if acceptance.is_empty() {
            return Err(ContractError::NoAcceptanceChecks);
        }

        Ok(Self {
            id: Uuid::new_v4().to_string(),
            prompt,
            acceptance,
            created_at: Utc::now(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CheckSpec {
    pub name: String,
    pub command: String,
    pub required: bool,
    pub timeout_secs: u64,
}

impl CheckSpec {
    pub fn required(
        name: impl Into<String>,
        command: impl Into<String>,
        timeout_secs: u64,
    ) -> Result<Self, ContractError> {
        Self::new(name, command, true, timeout_secs)
    }

    pub fn optional(
        name: impl Into<String>,
        command: impl Into<String>,
        timeout_secs: u64,
    ) -> Result<Self, ContractError> {
        Self::new(name, command, false, timeout_secs)
    }

    pub fn new(
        name: impl Into<String>,
        command: impl Into<String>,
        required: bool,
        timeout_secs: u64,
    ) -> Result<Self, ContractError> {
        let name = name.into();
        let command = command.into();

        if name.trim().is_empty() {
            return Err(ContractError::EmptyCheckName);
        }
        if command.trim().is_empty() {
            return Err(ContractError::EmptyCheckCommand);
        }
        if timeout_secs == 0 {
            return Err(ContractError::InvalidCheckTimeout);
        }

        Ok(Self {
            name,
            command,
            required,
            timeout_secs,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CheckResult {
    pub name: String,
    pub required: bool,
    pub passed: bool,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    pub output: String,
}

impl CheckResult {
    pub fn passed(
        name: impl Into<String>,
        required: bool,
        exit_code: Option<i32>,
        duration_ms: u64,
        output: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            required,
            passed: true,
            exit_code,
            duration_ms,
            output: output.into(),
        }
    }

    pub fn failed(
        name: impl Into<String>,
        required: bool,
        exit_code: Option<i32>,
        duration_ms: u64,
        output: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            required,
            passed: false,
            exit_code,
            duration_ms,
            output: output.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
    Renamed { from: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChangedPath {
    pub path: String,
    pub kind: ChangeKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChangeSet {
    pub files: Vec<ChangedPath>,
    pub diff_hash: Option<String>,
}

impl ChangeSet {
    pub fn new(
        mut files: Vec<ChangedPath>,
        diff_hash: Option<String>,
    ) -> Result<Self, ContractError> {
        let mut seen = HashSet::with_capacity(files.len());

        for changed in &files {
            validate_relative_path(&changed.path)?;
            if !seen.insert(changed.path.clone()) {
                return Err(ContractError::DuplicateChangedPath(changed.path.clone()));
            }

            if let ChangeKind::Renamed { from } = &changed.kind {
                validate_relative_path(from)?;
            }
        }

        files.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(Self { files, diff_hash })
    }

    pub fn empty() -> Self {
        Self {
            files: Vec::new(),
            diff_hash: None,
        }
    }

    pub fn has_changes(&self) -> bool {
        !self.files.is_empty()
    }

    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.files.iter().map(|changed| changed.path.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskStatus {
    Completed,
    Blocked,
    Failed,
    Interrupted,
    Inconclusive,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskResult {
    pub task_id: String,
    pub status: TaskStatus,
    pub summary: String,
    pub changes: Option<ChangeSet>,
    pub checks: Vec<CheckResult>,
    pub session_id: Option<String>,
    pub failure_reason: Option<String>,
    pub finished_at: DateTime<Utc>,
}

impl TaskResult {
    pub fn completed(
        task: &TaskSpec,
        summary: impl Into<String>,
        changes: ChangeSet,
        checks: Vec<CheckResult>,
        session_id: Option<String>,
    ) -> Result<Self, CompletionError> {
        let summary = summary.into();
        if summary.trim().is_empty() {
            return Err(CompletionError::EmptySummary);
        }
        if !changes.has_changes() {
            return Err(CompletionError::NoChanges);
        }

        for required_spec in task.acceptance.iter().filter(|check| check.required) {
            let matching = checks.iter().find(|result| result.name == required_spec.name);
            match matching {
                Some(result) if result.passed && result.required => {}
                Some(_) => {
                    let failed = checks
                        .iter()
                        .find(|result| result.name == required_spec.name)
                        .is_some_and(|result| !result.passed);
                    if failed {
                        return Err(CompletionError::RequiredCheckFailed(
                            required_spec.name.clone(),
                        ));
                    }
                    return Err(CompletionError::RequiredCheckMissing(
                        required_spec.name.clone(),
                    ));
                }
                None => {
                    return Err(CompletionError::RequiredCheckMissing(
                        required_spec.name.clone(),
                    ));
                }
            }
        }

        Ok(Self {
            task_id: task.id.clone(),
            status: TaskStatus::Completed,
            summary,
            changes: Some(changes),
            checks,
            session_id,
            failure_reason: None,
            finished_at: Utc::now(),
        })
    }

    pub fn blocked(
        task_id: impl Into<String>,
        summary: impl Into<String>,
        reason: impl Into<String>,
        session_id: Option<String>,
    ) -> Self {
        Self::non_completed(
            task_id,
            TaskStatus::Blocked,
            summary,
            Some(reason.into()),
            session_id,
        )
    }

    pub fn failed(
        task_id: impl Into<String>,
        summary: impl Into<String>,
        reason: impl Into<String>,
        session_id: Option<String>,
    ) -> Self {
        Self::non_completed(
            task_id,
            TaskStatus::Failed,
            summary,
            Some(reason.into()),
            session_id,
        )
    }

    pub fn inconclusive(
        task_id: impl Into<String>,
        summary: impl Into<String>,
        reason: impl Into<String>,
        session_id: Option<String>,
    ) -> Self {
        Self::non_completed(
            task_id,
            TaskStatus::Inconclusive,
            summary,
            Some(reason.into()),
            session_id,
        )
    }

    fn non_completed(
        task_id: impl Into<String>,
        status: TaskStatus,
        summary: impl Into<String>,
        failure_reason: Option<String>,
        session_id: Option<String>,
    ) -> Self {
        Self {
            task_id: task_id.into(),
            status,
            summary: summary.into(),
            changes: None,
            checks: Vec::new(),
            session_id,
            failure_reason,
            finished_at: Utc::now(),
        }
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ContractError {
    #[error("task prompt cannot be empty")]
    EmptyPrompt,
    #[error("task must define at least one acceptance check")]
    NoAcceptanceChecks,
    #[error("check name cannot be empty")]
    EmptyCheckName,
    #[error("check command cannot be empty")]
    EmptyCheckCommand,
    #[error("check timeout must be greater than zero")]
    InvalidCheckTimeout,
    #[error("path cannot be empty")]
    EmptyPath,
    #[error("path must be relative: {0}")]
    AbsolutePath(String),
    #[error("path contains parent traversal: {0}")]
    ParentTraversal(String),
    #[error("path touches protected .git data: {0}")]
    ProtectedPath(String),
    #[error("path does not name a file or directory: {0}")]
    InvalidPath(String),
    #[error("changed path appears more than once: {0}")]
    DuplicateChangedPath(String),
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CompletionError {
    #[error("completion summary cannot be empty")]
    EmptySummary,
    #[error("completed task must contain at least one changed path")]
    NoChanges,
    #[error("required acceptance check is missing: {0}")]
    RequiredCheckMissing(String),
    #[error("required acceptance check failed: {0}")]
    RequiredCheckFailed(String),
}

fn validate_relative_path(path: &str) -> Result<(), ContractError> {
    if path.trim().is_empty() {
        return Err(ContractError::EmptyPath);
    }

    let candidate = Path::new(path);
    if candidate.is_absolute() {
        return Err(ContractError::AbsolutePath(path.to_string()));
    }

    let mut has_normal_component = false;
    for component in candidate.components() {
        match component {
            Component::Normal(value) => {
                has_normal_component = true;
                if value == ".git" {
                    return Err(ContractError::ProtectedPath(path.to_string()));
                }
            }
            Component::ParentDir => {
                return Err(ContractError::ParentTraversal(path.to_string()));
            }
            Component::CurDir => {}
            Component::RootDir | Component::Prefix(_) => {
                return Err(ContractError::AbsolutePath(path.to_string()));
            }
        }
    }

    if !has_normal_component {
        return Err(ContractError::InvalidPath(path.to_string()));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check() -> CheckSpec {
        CheckSpec::required("unit", "cargo test -p example", 30).unwrap()
    }

    fn task() -> TaskSpec {
        TaskSpec::new("implement the requested change", vec![check()]).unwrap()
    }

    #[test]
    fn task_requires_prompt_and_acceptance() {
        assert_eq!(
            TaskSpec::new(" ", vec![check()]).unwrap_err(),
            ContractError::EmptyPrompt
        );
        assert_eq!(
            TaskSpec::new("task", Vec::new()).unwrap_err(),
            ContractError::NoAcceptanceChecks
        );
    }

    #[test]
    fn changed_paths_are_validated_and_sorted() {
        let changes = ChangeSet::new(
            vec![
                ChangedPath {
                    path: "src/z.rs".into(),
                    kind: ChangeKind::Modified,
                },
                ChangedPath {
                    path: "src/a.rs".into(),
                    kind: ChangeKind::Added,
                },
            ],
            None,
        )
        .unwrap();

        assert_eq!(changes.paths().collect::<Vec<_>>(), vec!["src/a.rs", "src/z.rs"]);
        assert!(ChangeSet::new(
            vec![ChangedPath {
                path: "../escape".into(),
                kind: ChangeKind::Added,
            }],
            None
        )
        .is_err());
        assert!(ChangeSet::new(
            vec![ChangedPath {
                path: ".git/config".into(),
                kind: ChangeKind::Modified,
            }],
            None
        )
        .is_err());
    }

    #[test]
    fn completion_requires_change_and_required_checks() {
        let task = task();
        assert_eq!(
            TaskResult::completed(
                &task,
                "done",
                ChangeSet::empty(),
                vec![CheckResult::passed("unit", true, Some(0), 10, "ok")],
                None,
            )
            .unwrap_err(),
            CompletionError::NoChanges
        );

        let changes = ChangeSet::new(
            vec![ChangedPath {
                path: "src/lib.rs".into(),
                kind: ChangeKind::Modified,
            }],
            Some("sha256:example".into()),
        )
        .unwrap();

        let result = TaskResult::completed(
            &task,
            "implemented",
            changes.clone(),
            vec![CheckResult::failed("unit", true, Some(1), 10, "failure")],
            Some("session-1".into()),
        );
        assert_eq!(
            result.unwrap_err(),
            CompletionError::RequiredCheckFailed("unit".into())
        );

        let result = TaskResult::completed(
            &task,
            "implemented",
            changes,
            vec![CheckResult::passed("unit", true, Some(0), 10, "ok")],
            Some("session-1".into()),
        )
        .unwrap();
        assert_eq!(result.status, TaskStatus::Completed);
    }

    #[test]
    fn failed_status_is_never_completed() {
        let result = TaskResult::failed("task-1", "cannot continue", "provider unavailable", None);
        assert_eq!(result.status, TaskStatus::Failed);
        assert_ne!(result.status, TaskStatus::Completed);
    }
}
