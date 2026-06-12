use std::path::PathBuf;
use std::time::Duration;

/// Configuration for sandbox execution.
#[derive(Debug, Clone)]
pub struct SandboxConfig {
    pub python_path: PathBuf,
    pub wall_timeout: Duration,
    pub cpu_limit_secs: u64,
    pub mem_limit_bytes: u64,
    pub fsize_limit_bytes: u64,
    pub nproc_limit: u64,
    pub nofile_limit: u64,
    pub stdout_cap: usize,
    pub stderr_cap: usize,
    pub use_netns: bool,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            python_path: PathBuf::from("python3"),
            wall_timeout: Duration::from_secs(10),
            cpu_limit_secs: 10,
            mem_limit_bytes: 512 * 1024 * 1024,
            fsize_limit_bytes: 10 * 1024 * 1024,
            nproc_limit: 64,
            nofile_limit: 64,
            stdout_cap: 256 * 1024,
            stderr_cap: 256 * 1024,
            use_netns: false,
        }
    }
}

/// Process execution status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecStatus {
    Success,
    NonZeroExit,
    Timeout,
    KilledBySignal,
    SpawnError,
}
