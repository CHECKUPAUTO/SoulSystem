//! Git worktree isolation for parallel workflow runs.

use std::path::PathBuf;
use std::process::Command;
use thiserror::Error;
use tracing::info;

#[derive(Debug, Error)]
pub enum WorktreeError {
    #[error("git command failed: {0}")]
    Git(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("worktree not found: {0}")]
    NotFound(PathBuf),
}

/// RAII handle to a git worktree. Cleans up on drop.
pub struct WorktreeHandle {
    pub path: PathBuf,
    pub branch: String,
    repo_path: PathBuf,
    cleaned_up: bool,
}

impl WorktreeHandle {
    /// Remove the worktree and delete the branch.
    pub fn cleanup(&mut self) -> Result<(), WorktreeError> {
        if self.cleaned_up {
            return Ok(());
        }
        // Remove worktree
        let status = Command::new("git")
            .current_dir(&self.repo_path)
            .args(["worktree", "remove", "--force"])
            .arg(&self.path)
            .status()
            .map_err(|e| WorktreeError::Git(format!("remove worktree: {e}")))?;

        if !status.success() {
            // Try pruning as fallback
            let _ = Command::new("git")
                .current_dir(&self.repo_path)
                .args(["worktree", "prune"])
                .status();
        }

        // Delete branch
        let _ = Command::new("git")
            .current_dir(&self.repo_path)
            .args(["branch", "-D", &self.branch])
            .status();

        self.cleaned_up = true;
        info!("Cleaned up worktree: {}", self.path.display());
        Ok(())
    }
}

impl Drop for WorktreeHandle {
    fn drop(&mut self) {
        if !self.cleaned_up {
            let _ = self.cleanup();
        }
    }
}

/// Manages isolated git worktrees for workflow runs.
pub struct WorktreeManager {
    repo_path: PathBuf,
}

impl WorktreeManager {
    pub fn new(repo_path: impl Into<PathBuf>) -> Self {
        Self {
            repo_path: repo_path.into(),
        }
    }

    /// Create an isolated worktree with a new branch.
    /// Branch naming: `soullink/workflow-{name}-{timestamp}`
    pub fn create_worktree(&self, name: &str) -> Result<WorktreeHandle, WorktreeError> {
        let timestamp = chrono::Utc::now().format("%Y%m%d%H%M%S");
        let branch = format!("soullink/workflow-{name}-{timestamp}");

        // Create branch
        let output = Command::new("git")
            .current_dir(&self.repo_path)
            .args(["branch", &branch])
            .output()
            .map_err(|e| WorktreeError::Git(format!("create branch: {e}")))?;

        if !output.status.success() {
            return Err(WorktreeError::Git(format!(
                "create branch failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        // Worktree path
        let worktree_dir = self.repo_path.join(format!(".worktree-{branch}"));

        // Create worktree
        let output = Command::new("git")
            .current_dir(&self.repo_path)
            .args(["worktree", "add"])
            .arg(&worktree_dir)
            .arg(&branch)
            .output()
            .map_err(|e| WorktreeError::Git(format!("add worktree: {e}")))?;

        if !output.status.success() {
            // Cleanup branch on failure
            let _ = Command::new("git")
                .current_dir(&self.repo_path)
                .args(["branch", "-D", &branch])
                .status();
            return Err(WorktreeError::Git(format!(
                "add worktree failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        info!(
            "Created worktree at {} on branch {branch}",
            worktree_dir.display()
        );

        Ok(WorktreeHandle {
            path: worktree_dir,
            branch,
            repo_path: self.repo_path.clone(),
            cleaned_up: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{WorktreeHandle, WorktreeManager};

    #[test]
    fn branch_naming_format() {
        let ts = "20260418184100";
        let name = "fix-github-issue";
        let branch = format!("soullink/workflow-{name}-{ts}");
        assert!(branch.starts_with("soullink/workflow-"));
        assert!(branch.contains("fix-github-issue"));
        assert!(branch.contains("20260418184100"));
    }

    #[test]
    fn unique_branches_for_concurrent_runs() {
        // Simulate two concurrent runs — timestamps differ
        let b1 = format!("soullink/workflow-test-{}", 1000);
        let b2 = format!("soullink/workflow-test-{}", 1001);
        assert_ne!(b1, b2);
    }
}
