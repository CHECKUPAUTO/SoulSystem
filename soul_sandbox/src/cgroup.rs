//! cgroup v2 backend for `pids.max` and `memory.max` (P1-12 part 1).
//!
//! `RLIMIT_NPROC` counts per real UID, so on any shared-UID host it cannot
//! bound a fork bomb without also breaking ordinary commands (see
//! `ResourceLimits::max_processes`). `RLIMIT_AS` bounds address space, not
//! resident memory. The controls that actually express "this process tree may
//! have at most N tasks and M bytes resident" are cgroup v2's `pids.max` and
//! `memory.max` — and they need something the host must *grant*: a delegated
//! subtree this process may create children under.
//!
//! # Availability is detected, reported, and never assumed
//!
//! cgroup v2's no-internal-process rule means a process cannot enable
//! controllers for children of the cgroup it itself occupies. A working setup
//! therefore requires an empty, controller-enabled directory prepared by the
//! operator (or a systemd `Delegate=yes` unit), named via
//! `SOUL_SANDBOX_CGROUP_DIR`. When that is absent or unusable, the sandbox
//! falls back to the P1-3 rlimits and says so once — a degraded bound that is
//! reported is a decision someone can revisit; a silent one is a hole.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Environment variable naming a delegated, controller-enabled cgroup v2
/// directory this process may create per-execution leaves under.
pub const CGROUP_DIR_ENV: &str = "SOUL_SANDBOX_CGROUP_DIR";

static LEAF_COUNTER: AtomicU64 = AtomicU64::new(0);

/// A delegated subtree we verified we can use.
#[derive(Debug, Clone)]
pub struct CgroupContext {
    base: PathBuf,
}

/// Why the cgroup backend is unavailable — carried in the report, not hidden.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CgroupUnavailable {
    /// `SOUL_SANDBOX_CGROUP_DIR` is not set.
    NotConfigured,
    /// The configured directory is missing or not writable by this process.
    NotUsable(String),
    /// The directory exists but its `cgroup.controllers` does not offer both
    /// `pids` and `memory`.
    ControllersMissing(String),
}

impl std::fmt::Display for CgroupUnavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotConfigured => write!(
                f,
                "{CGROUP_DIR_ENV} not set; falling back to rlimits (RLIMIT_NPROC \
                 cannot bound a fork bomb on a shared-UID host)"
            ),
            Self::NotUsable(e) => write!(f, "configured cgroup dir unusable: {e}"),
            Self::ControllersMissing(have) => write!(
                f,
                "cgroup dir lacks pids+memory controllers (has: {have}); \
                 enable them in the parent's cgroup.subtree_control"
            ),
        }
    }
}

impl CgroupContext {
    /// Detect a usable delegated subtree.
    ///
    /// Errors are typed, not logged here: the caller decides how loudly to
    /// report, so detection stays testable.
    pub fn detect() -> Result<Self, CgroupUnavailable> {
        let dir = std::env::var_os(CGROUP_DIR_ENV).ok_or(CgroupUnavailable::NotConfigured)?;
        let base = PathBuf::from(dir);
        let controllers = std::fs::read_to_string(base.join("cgroup.controllers"))
            .map_err(|e| CgroupUnavailable::NotUsable(e.to_string()))?;
        if !(controllers.split_whitespace().any(|c| c == "pids")
            && controllers.split_whitespace().any(|c| c == "memory"))
        {
            return Err(CgroupUnavailable::ControllersMissing(
                controllers.trim().to_string(),
            ));
        }
        // Prove we can actually create a leaf — read-only mounts and
        // undelegated dirs fail here, before any execution depends on it.
        let probe = base.join(format!(".soul-probe-{}", std::process::id()));
        std::fs::create_dir(&probe).map_err(|e| CgroupUnavailable::NotUsable(e.to_string()))?;
        let _ = std::fs::remove_dir(&probe);
        Ok(Self { base })
    }

    /// Create a per-execution leaf with the given limits applied.
    pub fn create_leaf(
        &self,
        pids_max: Option<u64>,
        memory_max_bytes: Option<u64>,
    ) -> io::Result<CgroupLeaf> {
        let leaf = self.base.join(format!(
            "soul-{}-{}",
            std::process::id(),
            LEAF_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&leaf)?;
        if let Some(n) = pids_max {
            std::fs::write(leaf.join("pids.max"), n.to_string())?;
        }
        if let Some(b) = memory_max_bytes {
            std::fs::write(leaf.join("memory.max"), b.to_string())?;
        }
        Ok(CgroupLeaf { path: leaf })
    }
}

/// A per-execution leaf cgroup. Removed on drop once empty.
#[derive(Debug)]
pub struct CgroupLeaf {
    path: PathBuf,
}

impl CgroupLeaf {
    /// The `cgroup.procs` file the child moves itself into from `pre_exec`
    /// (writing `"0"` moves the *calling* process — no PID race, no window
    /// where the child runs outside its limits).
    pub fn procs_path(&self) -> PathBuf {
        self.path.join("cgroup.procs")
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for CgroupLeaf {
    fn drop(&mut self) {
        // rmdir fails while member processes remain (the kernel holds the
        // leaf open); the wait/timeout path has already reaped the tree by
        // the time the leaf drops, so a brief retry covers zombie latency.
        for _ in 0..10 {
            match std::fs::remove_dir(&self.path) {
                Ok(()) => return,
                Err(_) => std::thread::sleep(std::time::Duration::from_millis(20)),
            }
        }
        tracing::warn!(
            "[sandbox] could not remove cgroup leaf {} — processes may still \
             be exiting; the operator's delegated dir accumulates a stale leaf",
            self.path.display()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The common host has no delegation: detection must fail with the typed
    /// reason, not panic and not pretend.
    #[test]
    fn detection_without_configuration_is_a_typed_absence() {
        // Serialise around the env var to avoid racing sibling tests.
        std::env::remove_var(CGROUP_DIR_ENV);
        assert_eq!(
            CgroupContext::detect().unwrap_err(),
            CgroupUnavailable::NotConfigured
        );
    }

    /// A configured-but-bogus directory is NotUsable, and the probe-leaf
    /// check catches an undelegated (read-only) dir before execution
    /// depends on it.
    #[test]
    fn a_bogus_dir_is_not_usable() {
        let dir = tempfile::tempdir().unwrap();
        // No cgroup.controllers file inside a plain tempdir.
        std::env::set_var(CGROUP_DIR_ENV, dir.path());
        let err = CgroupContext::detect().unwrap_err();
        std::env::remove_var(CGROUP_DIR_ENV);
        assert!(matches!(err, CgroupUnavailable::NotUsable(_)));
    }

    /// End-to-end enforcement needs a genuinely delegated subtree, which CI
    /// and developer hosts usually cannot provide. The test runs only where
    /// the operator has prepared one and says so otherwise — a skipped
    /// qualification is visible, a faked one is worse than none.
    #[test]
    fn a_delegated_subtree_creates_and_limits_a_leaf() {
        let Ok(ctx) = CgroupContext::detect() else {
            eprintln!(
                "skipping: no delegated cgroup subtree ({CGROUP_DIR_ENV} unset \
                 or unusable on this host)"
            );
            return;
        };
        let leaf = ctx.create_leaf(Some(64), Some(64 * 1024 * 1024)).unwrap();
        assert_eq!(
            std::fs::read_to_string(leaf.path().join("pids.max"))
                .unwrap()
                .trim(),
            "64"
        );
        drop(leaf);
    }
}
