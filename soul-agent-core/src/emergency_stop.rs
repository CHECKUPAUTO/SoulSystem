//! Global emergency-stop latch (INV-PLAN-3).
//!
//! [`EmergencyStop`] is a durable, file-backed latch. Its presence on disk
//! means every `AutonomousAgent` instance that shares the same latch path —
//! including one started in a *new* process after a restart — must refuse
//! new tool dispatch. Setting it ([`EmergencyStop::trip`]) is a single file
//! write; clearing it ([`EmergencyStop::operator_reset`]) requires an
//! explicit operator action. No agent-loop code calls `operator_reset` —
//! there is no automatic reset after a trip, by design.
//!
//! The latch is file-backed rather than an in-process flag (like
//! [`AutonomousAgent::abort`](crate::AutonomousAgent::abort), which only
//! cancels the current run of a single instance) specifically so it survives
//! a process restart: an operator who trips the latch and then restarts the
//! service must still find the agent halted.

use std::path::{Path, PathBuf};

/// A handle to a durable emergency-stop latch at a specific path.
///
/// Cheap to construct and clone-by-value (just a `PathBuf`); every method
/// re-reads the filesystem, so multiple independent `EmergencyStop` handles
/// pointing at the same path always agree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmergencyStop {
    path: PathBuf,
}

impl EmergencyStop {
    /// A handle to the latch file at an explicit path. Use this in tests (a
    /// per-test tempdir path) and anywhere the path must be controlled
    /// directly rather than read from the environment.
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// A handle to the latch at `SOULSYSTEM_ESTOP_PATH`, or a fixed default
    /// location if unset. This is what production code should use so every
    /// agent instance in the deployment shares one latch.
    pub fn from_env() -> Self {
        let path = std::env::var("SOULSYSTEM_ESTOP_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/var/lib/soulsystem/estop.latch"));
        Self { path }
    }

    /// The latch file path this handle points at.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Trip the latch, recording `reason` in the file. Idempotent: tripping
    /// an already-tripped latch overwrites the reason without erroring.
    pub fn trip(&self, reason: &str) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        std::fs::write(&self.path, reason)
    }

    /// Whether the latch is currently tripped.
    pub fn is_tripped(&self) -> bool {
        self.path.exists()
    }

    /// The reason recorded when the latch was tripped, if any.
    pub fn reason(&self) -> Option<String> {
        std::fs::read_to_string(&self.path).ok()
    }

    /// Explicit operator reset. Deliberately named to make call sites
    /// self-documenting; nothing in the agent loop calls this.
    pub fn operator_reset(&self) -> std::io::Result<()> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn handle() -> (tempfile::TempDir, EmergencyStop) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("estop.latch");
        (dir, EmergencyStop::at(path))
    }

    #[test]
    fn starts_untripped() {
        let (_dir, estop) = handle();
        assert!(!estop.is_tripped());
        assert_eq!(estop.reason(), None);
    }

    #[test]
    fn trip_sets_latch_and_reason() {
        let (_dir, estop) = handle();
        estop.trip("suspected prompt injection").unwrap();
        assert!(estop.is_tripped());
        assert_eq!(
            estop.reason().as_deref(),
            Some("suspected prompt injection")
        );
    }

    #[test]
    fn trip_is_idempotent_and_overwrites_reason() {
        let (_dir, estop) = handle();
        estop.trip("first reason").unwrap();
        estop.trip("second reason").unwrap();
        assert!(estop.is_tripped());
        assert_eq!(estop.reason().as_deref(), Some("second reason"));
    }

    #[test]
    fn operator_reset_clears_latch() {
        let (_dir, estop) = handle();
        estop.trip("test").unwrap();
        assert!(estop.is_tripped());
        estop.operator_reset().unwrap();
        assert!(!estop.is_tripped());
    }

    #[test]
    fn operator_reset_on_untripped_latch_is_a_noop() {
        let (_dir, estop) = handle();
        assert!(estop.operator_reset().is_ok());
        assert!(!estop.is_tripped());
    }

    #[test]
    fn independent_handles_to_the_same_path_agree() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("shared.latch");
        let a = EmergencyStop::at(&path);
        let b = EmergencyStop::at(&path);
        assert!(!b.is_tripped());
        a.trip("tripped via handle a").unwrap();
        assert!(b.is_tripped(), "handle b must observe handle a's trip");
        assert_eq!(b.reason().as_deref(), Some("tripped via handle a"));
    }

    #[test]
    fn creates_parent_directories_if_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("dir").join("estop.latch");
        let estop = EmergencyStop::at(path);
        estop.trip("nested").unwrap();
        assert!(estop.is_tripped());
    }

    #[test]
    fn trip_does_not_survive_only_in_memory_a_fresh_handle_sees_it() {
        // Proves durability across "restart": a brand-new handle (as a
        // fresh process would construct) observes a latch tripped earlier.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("restart.latch");
        {
            let estop = EmergencyStop::at(&path);
            estop.trip("pre-restart trip").unwrap();
        } // handle dropped — no in-memory state carries over
        let fresh = EmergencyStop::at(&path);
        assert!(fresh.is_tripped());
        assert_eq!(fresh.reason().as_deref(), Some("pre-restart trip"));
    }
}
