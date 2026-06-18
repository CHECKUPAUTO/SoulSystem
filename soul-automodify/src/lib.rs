//! Self-code modification with backup and rollback.
//!
//! Enables the agent to:
//! - Modify its own source code
//! - Create backups before changes
//! - Rollback to previous versions
//! - Track all self-modifications
//! - Validate changes compile before applying

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AutoModifyError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("File not found: {0}")]
    NotFound(String),
    #[error("Backup failed: {0}")]
    BackupFailed(String),
    #[error("Validation failed: {0}")]
    ValidationFailed(String),
    #[error("Rollback failed: {0}")]
    RollbackFailed(String),
    #[error("Patch failed: {0}")]
    PatchFailed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Modification {
    pub id: String,
    pub file_path: String,
    pub description: String,
    pub timestamp: DateTime<Utc>,
    pub backup_path: Option<String>,
    pub diff: String,
    pub applied: bool,
    pub verified: bool,
}

/// A compile-validation strategy: given the path just written, return `Ok(())`
/// if the change is acceptable, or `Err(reason)` to trigger a rollback.
type Validator = Box<dyn Fn(&Path) -> Result<(), String> + Send + Sync>;

pub struct AutoModifier {
    backup_dir: PathBuf,
    history: Vec<Modification>,
    validator: Validator,
}

impl AutoModifier {
    pub fn new(backup_dir: &Path) -> Self {
        Self {
            backup_dir: backup_dir.to_path_buf(),
            history: Vec::new(),
            validator: Box::new(default_cargo_validator),
        }
    }

    /// Replace the compile validator (e.g. for tests, or a non-cargo
    /// toolchain). The default runs `cargo check` on the modified file's crate.
    pub fn with_validator<F>(mut self, f: F) -> Self
    where
        F: Fn(&Path) -> Result<(), String> + Send + Sync + 'static,
    {
        self.validator = Box::new(f);
        self
    }

    /// Backup a file before modification
    pub fn backup(&self, file_path: &Path) -> Result<PathBuf, AutoModifyError> {
        if !file_path.exists() {
            return Err(AutoModifyError::NotFound(file_path.display().to_string()));
        }

        let filename = file_path
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| "backup".to_string());

        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let backup_name = format!("{}_{}.bak", filename, timestamp);
        let backup_path = self.backup_dir.join(&backup_name);

        std::fs::create_dir_all(&self.backup_dir)?;
        std::fs::copy(file_path, &backup_path)?;

        Ok(backup_path)
    }

    /// Apply a modification to a file.
    ///
    /// When `validate` is true the change is compile-checked *after* being
    /// written: if the validator rejects it, the write is rolled back (the
    /// original content is restored, or a newly-created file is removed) and
    /// [`AutoModifyError::ValidationFailed`] is returned — the agent never keeps
    /// a change that breaks the build. The resulting [`Modification::verified`]
    /// flag records whether validation actually ran and passed.
    pub fn modify(
        &mut self,
        file_path: &Path,
        new_content: &str,
        description: &str,
        validate: bool,
    ) -> Result<Modification, AutoModifyError> {
        // Read original content
        let existed = file_path.exists();
        let original = if existed {
            std::fs::read_to_string(file_path)?
        } else {
            String::new()
        };

        // Backup
        let backup_path = if existed {
            Some(self.backup(file_path)?.display().to_string())
        } else {
            None
        };

        // Compute diff
        let diff = compute_diff(&original, new_content);

        // Apply
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(file_path, new_content)?;

        // Compile-gate: validate the applied change, rolling back on failure so
        // a broken edit never survives.
        if validate {
            if let Err(reason) = (self.validator)(file_path) {
                if existed {
                    std::fs::write(file_path, &original)?;
                } else {
                    let _ = std::fs::remove_file(file_path);
                }
                return Err(AutoModifyError::ValidationFailed(reason));
            }
        }

        let mod_id = uuid::Uuid::new_v4().to_string();
        let modification = Modification {
            id: mod_id,
            file_path: file_path.display().to_string(),
            description: description.to_string(),
            timestamp: Utc::now(),
            backup_path,
            diff,
            applied: true,
            verified: validate,
        };

        self.history.push(modification.clone());

        Ok(modification)
    }

    /// Apply a patch (find and replace)
    pub fn patch_file(
        &mut self,
        file_path: &Path,
        find: &str,
        replace: &str,
        description: &str,
    ) -> Result<Modification, AutoModifyError> {
        let content = std::fs::read_to_string(file_path)?;

        if !content.contains(find) {
            return Err(AutoModifyError::PatchFailed(format!(
                "Pattern not found in {}",
                file_path.display()
            )));
        }

        let new_content = content.replace(find, replace);
        self.modify(file_path, &new_content, description, false)
    }

    /// Rollback a modification
    pub fn rollback(&mut self, modification_id: &str) -> Result<(), AutoModifyError> {
        let mod_entry = self
            .history
            .iter()
            .find(|m| m.id == modification_id)
            .cloned()
            .ok_or_else(|| AutoModifyError::NotFound(modification_id.to_string()))?;

        if let Some(ref backup_path) = mod_entry.backup_path {
            let backup = Path::new(backup_path);
            if backup.exists() {
                std::fs::copy(backup, &mod_entry.file_path)?;
                tracing::info!("Rolled back {} to {}", mod_entry.file_path, backup_path);
                Ok(())
            } else {
                Err(AutoModifyError::RollbackFailed(format!(
                    "Backup not found: {}",
                    backup_path
                )))
            }
        } else {
            Err(AutoModifyError::RollbackFailed(
                "No backup available (new file)".to_string(),
            ))
        }
    }

    /// Get modification history
    pub fn history(&self) -> &[Modification] {
        &self.history
    }

    /// Get recent modifications
    pub fn recent(&self, limit: usize) -> Vec<&Modification> {
        self.history.iter().rev().take(limit).collect()
    }

    /// Verify a file matches expected content
    pub fn verify(&self, file_path: &Path, expected: &str) -> Result<bool, AutoModifyError> {
        let actual = std::fs::read_to_string(file_path)?;
        Ok(actual == expected)
    }
}

/// Compute a simple diff between two strings
fn compute_diff(old: &str, new: &str) -> String {
    let mut diff = String::new();
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();

    let max_lines = old_lines.len().max(new_lines.len());

    for i in 0..max_lines {
        let old_line = old_lines.get(i).copied();
        let new_line = new_lines.get(i).copied();

        match (old_line, new_line) {
            (Some(o), Some(n)) if o != n => {
                diff.push_str(&format!("- {}\n+ {}\n", o, n));
            }
            (Some(o), None) => {
                diff.push_str(&format!("- {}\n", o));
            }
            (None, Some(n)) => {
                diff.push_str(&format!("+ {}\n", n));
            }
            _ => {}
        }
    }

    if diff.is_empty() {
        "No changes".to_string()
    } else {
        diff
    }
}

/// Walk up from `file_path` to the nearest ancestor directory containing a
/// `Cargo.toml`, returning that manifest path.
fn nearest_manifest(file_path: &Path) -> Option<PathBuf> {
    let mut dir = file_path.parent();
    while let Some(d) = dir {
        let candidate = d.join("Cargo.toml");
        if candidate.exists() {
            return Some(candidate);
        }
        dir = d.parent();
    }
    None
}

/// Default compile validator: run `cargo check` on the crate owning the
/// modified file.
///
/// Only Rust sources are compiled. A non-`.rs` file, or a `.rs` file with no
/// `Cargo.toml` ancestor (nothing to compile), passes — this is a build-quality
/// gate, not a security control, so "nothing to check" is success.
fn default_cargo_validator(file_path: &Path) -> Result<(), String> {
    if file_path.extension().and_then(|e| e.to_str()) != Some("rs") {
        return Ok(());
    }
    let Some(manifest) = nearest_manifest(file_path) else {
        return Ok(());
    };
    let output = std::process::Command::new("cargo")
        .arg("check")
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(&manifest)
        .output()
        .map_err(|e| format!("failed to run cargo check: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_backup_and_modify() {
        let dir = tempdir().unwrap();
        let backup_dir = dir.path().join("backups");
        let mut modifier = AutoModifier::new(&backup_dir);

        let file = dir.path().join("test.rs");
        std::fs::write(&file, "fn main() { println!(\"hello\"); }").unwrap();

        let modification = modifier
            .modify(
                &file,
                "fn main() { println!(\"world\"); }",
                "Changed greeting",
                false,
            )
            .unwrap();

        assert!(modification.applied);
        assert!(modification.backup_path.is_some());

        let content = std::fs::read_to_string(&file).unwrap();
        assert!(content.contains("world"));
    }

    #[test]
    fn validation_failure_rolls_back_existing_file() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("code.rs");
        std::fs::write(&file, "original").unwrap();

        // Validator that always rejects → the change must be rolled back.
        let mut modifier = AutoModifier::new(&dir.path().join("backups"))
            .with_validator(|_| Err("does not compile".to_string()));

        let result = modifier.modify(&file, "broken", "bad change", true);
        assert!(matches!(result, Err(AutoModifyError::ValidationFailed(_))));
        // File restored to the original content.
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "original");
        // Nothing recorded in history (the change did not stick).
        assert!(modifier.history().is_empty());
    }

    #[test]
    fn validation_failure_removes_new_file() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("new.rs");

        let mut modifier = AutoModifier::new(&dir.path().join("backups"))
            .with_validator(|_| Err("nope".to_string()));

        let result = modifier.modify(&file, "content", "create", true);
        assert!(result.is_err());
        // A newly-created file is removed on rollback.
        assert!(!file.exists());
    }

    #[test]
    fn validation_success_marks_verified() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("ok.rs");
        std::fs::write(&file, "old").unwrap();

        let mut modifier =
            AutoModifier::new(&dir.path().join("backups")).with_validator(|_| Ok(()));

        let m = modifier.modify(&file, "new", "good change", true).unwrap();
        assert!(m.verified);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "new");
    }

    #[test]
    fn test_rollback() {
        let dir = tempdir().unwrap();
        let backup_dir = dir.path().join("backups");
        let mut modifier = AutoModifier::new(&backup_dir);

        let file = dir.path().join("test.rs");
        std::fs::write(&file, "original").unwrap();

        let modification = modifier
            .modify(&file, "modified", "test change", false)
            .unwrap();

        modifier.rollback(&modification.id).unwrap();

        let content = std::fs::read_to_string(&file).unwrap();
        assert_eq!(content, "original");
    }

    #[test]
    fn test_patch_file() {
        let dir = tempdir().unwrap();
        let backup_dir = dir.path().join("backups");
        let mut modifier = AutoModifier::new(&backup_dir);

        let file = dir.path().join("config.txt");
        std::fs::write(&file, "model = qwen3:8b").unwrap();

        let _modification = modifier
            .patch_file(&file, "qwen3:8b", "gemma4:latest", "Update model")
            .unwrap();

        let content = std::fs::read_to_string(&file).unwrap();
        assert!(content.contains("gemma4:latest"));
    }

    #[test]
    fn test_compute_diff() {
        let diff = compute_diff("line1\nline2\nline3", "line1\nmodified\nline3\nline4");
        assert!(diff.contains("- line2"));
        assert!(diff.contains("+ modified"));
        assert!(diff.contains("+ line4"));
    }

    #[test]
    fn test_backup_creates_file() {
        let dir = tempdir().unwrap();
        let backup_dir = dir.path().join("backups");
        let modifier = AutoModifier::new(&backup_dir);

        let file = dir.path().join("test.rs");
        std::fs::write(&file, "content").unwrap();

        let backup_path = modifier.backup(&file).unwrap();
        assert!(backup_path.exists());
        let content = std::fs::read_to_string(&backup_path).unwrap();
        assert_eq!(content, "content");
    }

    #[test]
    fn test_backup_nonexistent_file() {
        let dir = tempdir().unwrap();
        let modifier = AutoModifier::new(&dir.path().join("backups"));
        let result = modifier.backup(&dir.path().join("nonexistent.rs"));
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_matches() {
        let dir = tempdir().unwrap();
        let modifier = AutoModifier::new(&dir.path().join("backups"));

        let file = dir.path().join("test.rs");
        std::fs::write(&file, "expected content").unwrap();

        assert!(modifier.verify(&file, "expected content").unwrap());
    }

    #[test]
    fn test_verify_mismatch() {
        let dir = tempdir().unwrap();
        let modifier = AutoModifier::new(&dir.path().join("backups"));

        let file = dir.path().join("test.rs");
        std::fs::write(&file, "actual content").unwrap();

        assert!(!modifier.verify(&file, "expected content").unwrap());
    }

    #[test]
    fn test_history_tracks_modifications() {
        let dir = tempdir().unwrap();
        let backup_dir = dir.path().join("backups");
        let mut modifier = AutoModifier::new(&backup_dir);

        let file = dir.path().join("test.rs");
        std::fs::write(&file, "original").unwrap();

        modifier.modify(&file, "v1", "first change", false).unwrap();
        modifier
            .modify(&file, "v2", "second change", false)
            .unwrap();

        assert_eq!(modifier.history().len(), 2);
        assert_eq!(modifier.history()[0].description, "first change");
        assert_eq!(modifier.history()[1].description, "second change");
    }

    #[test]
    fn test_recent_returns_latest_first() {
        let dir = tempdir().unwrap();
        let backup_dir = dir.path().join("backups");
        let mut modifier = AutoModifier::new(&backup_dir);

        let file = dir.path().join("test.rs");
        std::fs::write(&file, "original").unwrap();

        modifier.modify(&file, "a", "change a", false).unwrap();
        modifier.modify(&file, "b", "change b", false).unwrap();
        modifier.modify(&file, "c", "change c", false).unwrap();

        let recent = modifier.recent(2);
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].description, "change c");
        assert_eq!(recent[1].description, "change b");
    }

    #[test]
    fn test_patch_file_pattern_not_found() {
        let dir = tempdir().unwrap();
        let backup_dir = dir.path().join("backups");
        let mut modifier = AutoModifier::new(&backup_dir);

        let file = dir.path().join("config.txt");
        std::fs::write(&file, "model = qwen3:8b").unwrap();

        let result = modifier.patch_file(&file, "nonexistent", "replacement", "bad patch");
        assert!(result.is_err());
    }

    #[test]
    fn test_rollback_without_backup_fails() {
        let dir = tempdir().unwrap();
        let backup_dir = dir.path().join("backups");
        let mut modifier = AutoModifier::new(&backup_dir);

        let file = dir.path().join("new.rs");
        let modification = modifier
            .modify(&file, "new content", "new file creation", false)
            .unwrap();

        let result = modifier.rollback(&modification.id);
        assert!(result.is_err());
    }

    #[test]
    fn test_rollback_nonexistent_modification() {
        let dir = tempdir().unwrap();
        let mut modifier = AutoModifier::new(&dir.path().join("backups"));

        let result = modifier.rollback("nonexistent-id");
        assert!(result.is_err());
    }

    #[test]
    fn test_compute_diff_no_changes() {
        let diff = compute_diff("same line", "same line");
        assert_eq!(diff, "No changes");
    }

    #[test]
    fn test_compute_diff_added_lines() {
        let diff = compute_diff("line1", "line1\nline2\nline3");
        assert!(diff.contains("+ line2"));
        assert!(diff.contains("+ line3"));
    }

    #[test]
    fn test_compute_diff_removed_lines() {
        let diff = compute_diff("line1\nline2\nline3", "line1");
        assert!(diff.contains("- line2"));
        assert!(diff.contains("- line3"));
    }
}
