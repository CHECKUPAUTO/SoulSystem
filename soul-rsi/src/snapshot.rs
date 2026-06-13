//! Isolated workspace snapshots.
//!
//! Every candidate is evaluated against a *copy* of the codebase, never the
//! live tree. This is the safety boundary that makes self-modification
//! acceptable to run unattended: a proposal that corrupts the build can only
//! corrupt a throwaway temp directory. Promotion to the live tree is a separate,
//! explicit step gated on a passing evaluation.

use crate::error::{Result, RsiError};
use crate::variant::Patch;
use soul_automodify::AutoModifier;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Directories never worth copying into a snapshot (huge and regenerated).
const SKIP_DIRS: &[&str] = &["target", ".git", "node_modules"];

/// A throwaway copy of a workspace root with (optionally) a patch applied.
/// The underlying temp directory is removed when this value is dropped.
pub struct WorkspaceSnapshot {
    _tmp: TempDir,
    root: PathBuf,
}

impl WorkspaceSnapshot {
    /// Recursively copy `src_root` into a fresh temp directory.
    pub fn create(src_root: &Path) -> Result<Self> {
        let tmp = TempDir::new()?;
        let root = tmp.path().join("workspace");
        std::fs::create_dir_all(&root)?;
        copy_tree(src_root, &root)?;
        Ok(Self { _tmp: tmp, root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn resolve(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    /// Apply a patch to the snapshot via `soul_automodify` (so the edit is
    /// backed up and recorded inside the snapshot just as a live edit would be).
    pub fn apply(&self, patch: &Patch) -> Result<()> {
        if patch.is_noop() {
            return Err(RsiError::Apply("patch is a no-op".to_string()));
        }
        let target = self.resolve(&patch.target);
        if !target.starts_with(&self.root) {
            return Err(RsiError::PathNotAllowed(patch.target.clone()));
        }
        let backups = self.root.join(".rsi_backups");
        let mut modifier = AutoModifier::new(&backups);
        modifier
            .patch_file(&target, &patch.find, &patch.replace, "rsi candidate")
            .map_err(|e| RsiError::Apply(e.to_string()))?;
        Ok(())
    }
}

/// Apply an accepted patch to the *live* tree, with a backup, returning the
/// modification id. This is the only function that mutates the real codebase
/// and callers gate it on a passing, all-green evaluation.
pub fn promote_to_live(live_root: &Path, patch: &Patch, backup_dir: &Path) -> Result<String> {
    let target = live_root.join(&patch.target);
    let mut modifier = AutoModifier::new(backup_dir);
    let m = modifier
        .patch_file(&target, &patch.find, &patch.replace, "rsi promotion")
        .map_err(|e| RsiError::Apply(e.to_string()))?;
    Ok(m.id)
}

fn copy_tree(src: &Path, dst: &Path) -> Result<()> {
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        let ty = entry.file_type()?;
        let to = dst.join(&name);
        if ty.is_dir() {
            if SKIP_DIRS.contains(&name_str.as_ref()) {
                continue;
            }
            std::fs::create_dir_all(&to)?;
            copy_tree(&entry.path(), &to)?;
        } else if ty.is_file() {
            std::fs::copy(entry.path(), &to)?;
        }
        // symlinks and other node types are intentionally skipped.
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, rel: &str, content: &str) {
        let p = root.join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(p, content).unwrap();
    }

    #[test]
    fn snapshot_copies_and_isolates() {
        let live = TempDir::new().unwrap();
        write(live.path(), "src/lib.rs", "fn main() { let x = 0; }");
        write(live.path(), "target/junk.o", "binary"); // must be skipped

        let snap = WorkspaceSnapshot::create(live.path()).unwrap();
        assert!(snap.resolve("src/lib.rs").exists());
        assert!(!snap.resolve("target/junk.o").exists());

        // Patching the snapshot must not touch the live tree.
        snap.apply(&Patch::new("src/lib.rs", "let x = 0;", "let x = 1;"))
            .unwrap();
        let live_src = std::fs::read_to_string(live.path().join("src/lib.rs")).unwrap();
        assert!(live_src.contains("let x = 0;"), "live tree was mutated");
        let snap_src = std::fs::read_to_string(snap.resolve("src/lib.rs")).unwrap();
        assert!(snap_src.contains("let x = 1;"));
    }

    #[test]
    fn noop_patch_is_rejected() {
        let live = TempDir::new().unwrap();
        write(live.path(), "a.rs", "x");
        let snap = WorkspaceSnapshot::create(live.path()).unwrap();
        assert!(snap.apply(&Patch::new("a.rs", "x", "x")).is_err());
    }

    #[test]
    fn promote_writes_live_tree() {
        let live = TempDir::new().unwrap();
        write(live.path(), "a.rs", "value = 1");
        let backups = TempDir::new().unwrap();
        promote_to_live(
            live.path(),
            &Patch::new("a.rs", "value = 1", "value = 2"),
            backups.path(),
        )
        .unwrap();
        let after = std::fs::read_to_string(live.path().join("a.rs")).unwrap();
        assert_eq!(after, "value = 2");
    }
}
