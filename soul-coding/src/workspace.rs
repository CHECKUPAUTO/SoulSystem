//! Workspace identity and path confinement.
//!
//! The first coding-harness invariant is that every file identity is relative
//! to a known worktree. This module resolves existing symlinks before checking
//! containment and rejects .git paths and parent traversal.

use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::fs;
use std::path::{Component, Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceContext {
    root: PathBuf,
    worktree: PathBuf,
    base_revision: String,
    session_id: String,
}

impl WorkspaceContext {
    pub fn new(
        root: impl AsRef<Path>,
        worktree: impl AsRef<Path>,
        base_revision: impl Into<String>,
        session_id: impl Into<String>,
    ) -> Result<Self, WorkspaceError> {
        let root = fs::canonicalize(root.as_ref()).map_err(|error| WorkspaceError::Io {
            path: root.as_ref().display().to_string(),
            detail: error.to_string(),
        })?;
        if !root.is_dir() {
            return Err(WorkspaceError::NotDirectory(root.display().to_string()));
        }

        let worktree = fs::canonicalize(worktree.as_ref()).map_err(|error| WorkspaceError::Io {
            path: worktree.as_ref().display().to_string(),
            detail: error.to_string(),
        })?;
        if !worktree.is_dir() {
            return Err(WorkspaceError::NotDirectory(worktree.display().to_string()));
        }
        if !worktree.starts_with(&root) {
            return Err(WorkspaceError::WorktreeOutsideRoot {
                root: root.display().to_string(),
                worktree: worktree.display().to_string(),
            });
        }

        let base_revision = base_revision.into();
        if base_revision.trim().is_empty() {
            return Err(WorkspaceError::EmptyBaseRevision);
        }

        let session_id = session_id.into();
        if session_id.trim().is_empty() {
            return Err(WorkspaceError::EmptySessionId);
        }

        Ok(Self {
            root,
            worktree,
            base_revision,
            session_id,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn worktree(&self) -> &Path {
        &self.worktree
    }

    pub fn base_revision(&self) -> &str {
        &self.base_revision
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Resolve a worktree-relative path without following an escape symlink.
    pub fn resolve_path(&self, relative: &str) -> Result<PathBuf, WorkspaceError> {
        validate_relative_path(relative)?;

        let requested = Path::new(relative);
        let candidate = self.worktree.join(requested);
        self.validate_symlink_components(&requested, relative)?;
        let mut existing = candidate.clone();
        let mut tail: Vec<OsString> = Vec::new();

        while !existing.exists() {
            let parent = existing
                .parent()
                .ok_or_else(|| WorkspaceError::OutsideWorktree(relative.to_string()))?;
            if let Some(name) = existing.file_name() {
                tail.push(name.to_os_string());
            }
            existing = parent.to_path_buf();
        }

        let canonical_existing =
            fs::canonicalize(&existing).map_err(|error| WorkspaceError::Io {
                path: relative.to_string(),
                detail: error.to_string(),
            })?;

        if !canonical_existing.starts_with(&self.worktree) {
            return Err(WorkspaceError::OutsideWorktree(relative.to_string()));
        }

        let mut resolved = canonical_existing;
        for component in tail.into_iter().rev() {
            resolved.push(component);
        }

        if !resolved.starts_with(&self.worktree) {
            return Err(WorkspaceError::OutsideWorktree(relative.to_string()));
        }

        Ok(resolved)
    }

    /// Check symlink components before looking for the nearest existing
    /// parent. `Path::exists()` returns false for a broken symlink, so a
    /// missing-target link would otherwise be mistaken for an ordinary new
    /// path and a later write could escape the worktree.
    fn validate_symlink_components(
        &self,
        requested: &Path,
        display_path: &str,
    ) -> Result<(), WorkspaceError> {
        let mut current = self.worktree.clone();

        for component in requested.components() {
            let Component::Normal(name) = component else {
                continue;
            };
            current.push(name);

            match fs::symlink_metadata(&current) {
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    let canonical = fs::canonicalize(&current).map_err(|_| {
                        WorkspaceError::OutsideWorktree(display_path.to_string())
                    })?;
                    if !canonical.starts_with(&self.worktree) {
                        return Err(WorkspaceError::OutsideWorktree(
                            display_path.to_string(),
                        ));
                    }
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
                Err(error) => {
                    return Err(WorkspaceError::Io {
                        path: display_path.to_string(),
                        detail: error.to_string(),
                    });
                }
            }
        }

        Ok(())
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum WorkspaceError {
    #[error("workspace path is not a directory: {0}")]
    NotDirectory(String),
    #[error("worktree {worktree} is outside workspace root {root}")]
    WorktreeOutsideRoot { root: String, worktree: String },
    #[error("base revision cannot be empty")]
    EmptyBaseRevision,
    #[error("session id cannot be empty")]
    EmptySessionId,
    #[error("workspace path cannot be empty")]
    EmptyPath,
    #[error("workspace path must be relative: {0}")]
    AbsolutePath(String),
    #[error("workspace path contains parent traversal: {0}")]
    ParentTraversal(String),
    #[error("workspace path touches protected .git data: {0}")]
    ProtectedPath(String),
    #[error("workspace path is invalid: {0}")]
    InvalidPath(String),
    #[error("workspace path escapes worktree: {0}")]
    OutsideWorktree(String),
    #[error("workspace I/O error for {path}: {detail}")]
    Io { path: String, detail: String },
}

fn validate_relative_path(relative: &str) -> Result<(), WorkspaceError> {
    if relative.trim().is_empty() {
        return Err(WorkspaceError::EmptyPath);
    }

    let candidate = Path::new(relative);
    if candidate.is_absolute() {
        return Err(WorkspaceError::AbsolutePath(relative.to_string()));
    }

    let mut has_normal_component = false;
    for component in candidate.components() {
        match component {
            Component::Normal(value) => {
                has_normal_component = true;
                if value == ".git" {
                    return Err(WorkspaceError::ProtectedPath(relative.to_string()));
                }
            }
            Component::ParentDir => {
                return Err(WorkspaceError::ParentTraversal(relative.to_string()));
            }
            Component::CurDir => {}
            Component::RootDir | Component::Prefix(_) => {
                return Err(WorkspaceError::AbsolutePath(relative.to_string()));
            }
        }
    }

    if !has_normal_component {
        return Err(WorkspaceError::InvalidPath(relative.to_string()));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(root: &Path) -> WorkspaceContext {
        WorkspaceContext::new(root, root, "base-sha", "session-1").unwrap()
    }

    #[test]
    fn resolves_existing_and_new_paths_inside_worktree() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("src");
        fs::create_dir_all(&file).unwrap();
        fs::write(file.join("lib.rs"), "fn main() {}").unwrap();

        let context = context(dir.path());
        assert_eq!(
            context.resolve_path("src/lib.rs").unwrap(),
            file.join("lib.rs").canonicalize().unwrap()
        );

        let new_path = context.resolve_path("src/new.rs").unwrap();
        assert_eq!(new_path, file.join("new.rs"));
    }

    #[test]
    fn rejects_escape_and_protected_paths() {
        let dir = tempfile::tempdir().unwrap();
        let context = context(dir.path());

        assert!(matches!(
            context.resolve_path("../outside"),
            Err(WorkspaceError::ParentTraversal(_))
        ));
        assert!(matches!(
            context.resolve_path("/etc/passwd"),
            Err(WorkspaceError::AbsolutePath(_))
        ));
        assert!(matches!(
            context.resolve_path(".git/config"),
            Err(WorkspaceError::ProtectedPath(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escape_for_existing_and_new_files() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("secret.txt"), "secret").unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("linked")).unwrap();

        let context = context(root.path());

        assert!(matches!(
            context.resolve_path("linked/secret.txt"),
            Err(WorkspaceError::OutsideWorktree(_))
        ));
        assert!(matches!(
            context.resolve_path("linked/new.txt"),
            Err(WorkspaceError::OutsideWorktree(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_broken_symlink_before_a_new_file_can_escape() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let link = root.path().join("linked");
        std::os::unix::fs::symlink(outside.path().join("missing"), &link).unwrap();

        let context = context(root.path());

        assert!(matches!(
            context.resolve_path("linked/new.txt"),
            Err(WorkspaceError::OutsideWorktree(_))
        ));
    }

    #[test]
    fn rejects_worktree_outside_root() {
        let root = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();

        assert!(matches!(
            WorkspaceContext::new(root.path(), other.path(), "sha", "session"),
            Err(WorkspaceError::WorktreeOutsideRoot { .. })
        ));
    }
}
