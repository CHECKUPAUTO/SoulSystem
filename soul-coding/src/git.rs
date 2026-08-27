//! Git-backed worktree identity and change-set collection.

use crate::command::CommandSpec;
use crate::contract::{ChangeKind, ChangeSet};
use crate::runner::{CommandOutput, CommandRunner, RunnerError, SandboxCommandRunner};
use crate::workspace::{WorkspaceContext, WorkspaceError};
use sha2::{Digest, Sha256};
use soul_sandbox::SandboxPolicy;
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;
use thiserror::Error;

pub struct GitWorkspace<R> {
    context: WorkspaceContext,
    runner: R,
    command_timeout: Duration,
}

impl<R> GitWorkspace<R>
where
    R: CommandRunner,
{
    pub fn from_context(context: WorkspaceContext, runner: R) -> Self {
        Self {
            context,
            runner,
            command_timeout: Duration::from_secs(30),
        }
    }

    pub fn with_command_timeout(mut self, timeout: Duration) -> Result<Self, GitError> {
        if timeout.is_zero() {
            return Err(GitError::InvalidTimeout);
        }
        self.command_timeout = timeout;
        Ok(self)
    }

    pub fn context(&self) -> &WorkspaceContext {
        &self.context
    }

    pub fn status(&self) -> Result<Vec<crate::contract::ChangedPath>, GitError> {
        let output = self.run_git(
            self.context.worktree(),
            CommandSpec::new("git")?
                .flag("-C")
                .value(self.context.worktree().display().to_string())
                .flag("status")
                .flag("--porcelain=v1")
                .flag("-z")
                .flag("--untracked-files=all"),
        )?;
        ensure_success("git status", output)
    }

    pub fn change_set(&self) -> Result<ChangeSet, GitError> {
        let status = self.status()?;
        let status_bytes = serialize_paths(&status);
        let diff = self.run_git(
            self.context.worktree(),
            CommandSpec::new("git")?
                .flag("-C")
                .value(self.context.worktree().display().to_string())
                .flag("diff")
                .flag("--no-ext-diff")
                .flag("--no-color")
                .flag("--binary")
                .flag("HEAD"),
        )?;
        let diff = ensure_output_success("git diff", diff)?;

        let mut hasher = Sha256::new();
        hasher.update(status_bytes);
        hasher.update(diff.stdout.as_bytes());
        for changed in &status {
            hash_path_state(&mut hasher, &self.context, &changed.path)?;
            if let ChangeKind::Renamed { from } = &changed.kind {
                hash_path_state(&mut hasher, &self.context, from)?;
            }
        }
        let diff_hash = format!("sha256:{:x}", hasher.finalize());

        ChangeSet::new(status, Some(diff_hash)).map_err(GitError::Contract)
    }

    fn run_git(&self, working_dir: &Path, command: CommandSpec) -> Result<CommandOutput, GitError> {
        self.runner
            .run(&command, working_dir, self.command_timeout)
            .map_err(GitError::Runner)
    }
}

impl GitWorkspace<SandboxCommandRunner> {
    /// Re-open an existing detached worktree for a resumable session.
    pub fn open(
        root: impl AsRef<Path>,
        base_revision: impl Into<String>,
        session_id: impl Into<String>,
        policy: SandboxPolicy,
    ) -> Result<Self, GitError> {
        let root = fs::canonicalize(root.as_ref()).map_err(|error| GitError::Io {
            path: root.as_ref().display().to_string(),
            detail: error.to_string(),
        })?;
        if !root.is_dir() {
            return Err(GitError::NotDirectory(root.display().to_string()));
        }

        let base_revision = base_revision.into();
        if base_revision.trim().is_empty() {
            return Err(GitError::EmptyBaseRevision);
        }
        let session_id = session_id.into();
        validate_session_id(&session_id)?;

        let worktree = root.join(".soul").join("worktrees").join(&session_id);
        if !worktree.is_dir() {
            return Err(GitError::WorktreeNotFound(worktree.display().to_string()));
        }
        let context = WorkspaceContext::new(&root, &worktree, base_revision, session_id)
            .map_err(GitError::Workspace)?;
        let runner = SandboxCommandRunner::new(SandboxPolicy {
            working_dir: Some(root),
            ..policy
        });
        Ok(Self::from_context(context, runner))
    }

    /// Create an isolated detached worktree under `.soul/worktrees`.
    ///
    /// The parent directory is created by Rust inside the canonical repository
    /// root; Git itself remains the only process that creates or removes a
    /// worktree. The returned context is the identity used by all later file
    /// and verification operations.
    pub fn create(
        root: impl AsRef<Path>,
        base_revision: impl Into<String>,
        session_id: impl Into<String>,
        policy: SandboxPolicy,
    ) -> Result<Self, GitError> {
        let root = fs::canonicalize(root.as_ref()).map_err(|error| GitError::Io {
            path: root.as_ref().display().to_string(),
            detail: error.to_string(),
        })?;
        if !root.is_dir() {
            return Err(GitError::NotDirectory(root.display().to_string()));
        }

        let base_revision = base_revision.into();
        if base_revision.trim().is_empty() {
            return Err(GitError::EmptyBaseRevision);
        }
        let session_id = session_id.into();
        validate_session_id(&session_id)?;

        let worktree = root.join(".soul").join("worktrees").join(&session_id);
        if worktree.exists() {
            return Err(GitError::WorktreeAlreadyExists(
                worktree.display().to_string(),
            ));
        }
        fs::create_dir_all(worktree.parent().expect("worktree has a parent")).map_err(|error| {
            GitError::Io {
                path: worktree.display().to_string(),
                detail: error.to_string(),
            }
        })?;

        let runner = SandboxCommandRunner::new(SandboxPolicy {
            working_dir: Some(root.clone()),
            ..policy
        });
        let command = CommandSpec::new("git")?
            .flag("-C")
            .value(root.display().to_string())
            .flag("worktree")
            .flag("add")
            .flag("--detach")
            .flag("--")
            .value(worktree.display().to_string())
            .value(base_revision.clone());
        let output = runner
            .run(&command, &root, Duration::from_secs(60))
            .map_err(GitError::Runner)?;
        ensure_output_success("git worktree add", output)?;

        let context = WorkspaceContext::new(&root, &worktree, base_revision, session_id)
            .map_err(GitError::Workspace)?;
        Ok(Self::from_context(context, runner))
    }
}

fn validate_session_id(session_id: &str) -> Result<(), GitError> {
    if session_id.trim().is_empty() {
        return Err(GitError::EmptySessionId);
    }
    let mut components = Path::new(session_id).components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(_)), None) => Ok(()),
        _ => Err(GitError::InvalidSessionId(session_id.to_string())),
    }
}

fn ensure_success(
    operation: &str,
    output: CommandOutput,
) -> Result<Vec<crate::contract::ChangedPath>, GitError> {
    if output.exit_code != Some(0) || output.timed_out {
        return Err(GitError::CommandFailed {
            operation: operation.to_string(),
            exit_code: output.exit_code,
            output: merge_output(&output),
        });
    }
    parse_status(&output.stdout)
}

fn ensure_output_success(
    operation: &str,
    output: CommandOutput,
) -> Result<CommandOutput, GitError> {
    if output.exit_code != Some(0) || output.timed_out {
        return Err(GitError::CommandFailed {
            operation: operation.to_string(),
            exit_code: output.exit_code,
            output: merge_output(&output),
        });
    }
    Ok(output)
}

fn merge_output(output: &CommandOutput) -> String {
    match (output.stdout.is_empty(), output.stderr.is_empty()) {
        (true, true) => "(no output)".into(),
        (false, true) => output.stdout.clone(),
        (true, false) => output.stderr.clone(),
        (false, false) => format!("{}\n{}", output.stdout, output.stderr),
    }
}

fn parse_status(output: &str) -> Result<Vec<crate::contract::ChangedPath>, GitError> {
    let bytes = output.as_bytes();
    let mut records = bytes.split(|byte| *byte == 0);
    let mut changes = Vec::new();

    while let Some(record) = records.next() {
        if record.is_empty() {
            continue;
        }
        if record.len() < 4 || record[2] != b' ' {
            return Err(GitError::MalformedStatus(
                String::from_utf8_lossy(record).into(),
            ));
        }

        let status = [record[0] as char, record[1] as char];
        let path = String::from_utf8(record[3..].to_vec())
            .map_err(|_| GitError::NonUtf8Path(String::from_utf8_lossy(&record[3..]).into()))?;
        let kind = if status.contains(&'?') || status.contains(&'A') {
            ChangeKind::Added
        } else if status.contains(&'D') {
            ChangeKind::Deleted
        } else if status.contains(&'R') || status.contains(&'C') {
            let previous = records
                .next()
                .ok_or_else(|| GitError::MalformedStatus(path.clone()))?;
            let previous = String::from_utf8(previous.to_vec())
                .map_err(|_| GitError::NonUtf8Path(String::from_utf8_lossy(previous).into()))?;
            ChangeKind::Renamed { from: previous }
        } else {
            ChangeKind::Modified
        };
        changes.push(crate::contract::ChangedPath { path, kind });
    }

    Ok(changes)
}

fn serialize_paths(paths: &[crate::contract::ChangedPath]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for path in paths {
        bytes.extend_from_slice(path.path.as_bytes());
        bytes.push(0);
    }
    bytes
}

fn hash_path_state(
    hasher: &mut Sha256,
    context: &WorkspaceContext,
    relative: &str,
) -> Result<(), GitError> {
    let resolved = context.resolve_path(relative)?;
    let candidate = context.worktree().join(relative);
    hasher.update(b"path\0");
    hasher.update(relative.as_bytes());
    hasher.update(b"\0");

    match fs::symlink_metadata(&candidate) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            hasher.update(b"symlink\0");
            let target = fs::read_link(&candidate).map_err(|error| GitError::Io {
                path: relative.to_string(),
                detail: error.to_string(),
            })?;
            hasher.update(target.to_string_lossy().as_bytes());
            hash_resolved_file(hasher, &resolved)?;
        }
        Ok(metadata) if metadata.is_file() => {
            hasher.update(b"file\0");
            hash_resolved_file(hasher, &resolved)?;
        }
        Ok(metadata) => {
            hasher.update(b"other\0");
            hasher.update(metadata.len().to_le_bytes());
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            hasher.update(b"missing\0");
        }
        Err(error) => {
            return Err(GitError::Io {
                path: relative.to_string(),
                detail: error.to_string(),
            });
        }
    }
    Ok(())
}

fn hash_resolved_file(hasher: &mut Sha256, path: &Path) -> Result<(), GitError> {
    let mut file = fs::File::open(path).map_err(|error| GitError::Io {
        path: path.display().to_string(),
        detail: error.to_string(),
    })?;
    let mut buffer = [0_u8; 8192];
    loop {
        let read = file.read(&mut buffer).map_err(|error| GitError::Io {
            path: path.display().to_string(),
            detail: error.to_string(),
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(())
}

#[derive(Debug, Error)]
pub enum GitError {
    #[error("Git command failed during {operation} (exit code {exit_code:?}): {output}")]
    CommandFailed {
        operation: String,
        exit_code: Option<i32>,
        output: String,
    },
    #[error("Git status output is malformed: {0}")]
    MalformedStatus(String),
    #[error("Git returned a non-UTF-8 path: {0}")]
    NonUtf8Path(String),
    #[error("Git workspace path is not a directory: {0}")]
    NotDirectory(String),
    #[error("Git workspace already exists: {0}")]
    WorktreeAlreadyExists(String),
    #[error("Git worktree was not found for session: {0}")]
    WorktreeNotFound(String),
    #[error("Git base revision cannot be empty")]
    EmptyBaseRevision,
    #[error("Git session id cannot be empty")]
    EmptySessionId,
    #[error("Git session id must be one safe path component: {0}")]
    InvalidSessionId(String),
    #[error("Git command timeout must be greater than zero")]
    InvalidTimeout,
    #[error("Git filesystem error for {path}: {detail}")]
    Io { path: String, detail: String },
    #[error("workspace error: {0}")]
    Workspace(#[from] WorkspaceError),
    #[error("contract error: {0}")]
    Contract(#[from] crate::contract::ContractError),
    #[error("runner error: {0}")]
    Runner(#[from] RunnerError),
    #[error("command specification error: {0}")]
    CommandSpec(#[from] crate::command::CommandSpecError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::CommandOutput;
    use std::sync::Mutex;

    #[derive(Clone)]
    struct FakeRunner {
        commands: std::sync::Arc<Mutex<Vec<String>>>,
        output: CommandOutput,
    }

    impl CommandRunner for FakeRunner {
        fn run(
            &self,
            command: &CommandSpec,
            _working_dir: &Path,
            _timeout: Duration,
        ) -> Result<CommandOutput, RunnerError> {
            self.commands.lock().unwrap().push(command.display());
            Ok(self.output.clone())
        }
    }

    #[test]
    fn parses_porcelain_status_into_a_stable_change_set() {
        let output = " M src/lib.rs\0?? src/new.rs\0D  src/old.rs\0";
        let changes = parse_status(output).unwrap();
        assert_eq!(changes.len(), 3);
        assert!(matches!(changes[0].kind, ChangeKind::Modified));
        assert!(matches!(changes[1].kind, ChangeKind::Added));
        assert!(matches!(changes[2].kind, ChangeKind::Deleted));
    }

    #[test]
    fn status_collects_only_after_a_successful_git_command() {
        let dir = tempfile::tempdir().unwrap();
        let runner = FakeRunner {
            commands: Default::default(),
            output: CommandOutput {
                stdout: "?? src/new.rs\0".into(),
                stderr: String::new(),
                exit_code: Some(0),
                duration_ms: 1,
                timed_out: false,
            },
        };
        let context = WorkspaceContext::new(dir.path(), dir.path(), "base", "session").unwrap();
        let workspace = GitWorkspace::from_context(context, runner);
        let changes = workspace.status().unwrap();
        assert_eq!(changes[0].path, "src/new.rs");
    }
}
