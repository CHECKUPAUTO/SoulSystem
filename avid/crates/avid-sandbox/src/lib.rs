#![deny(unsafe_code)]
#![warn(clippy::pedantic, clippy::nursery)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]
#![allow(clippy::module_name_repetitions)]

//! Hardened subprocess sandbox for executing untrusted Python code.
//!
//! Applies kernel-enforced resource limits (rlimit), privilege
//! restrictions (`PR_SET_NO_NEW_PRIVS`), process group isolation,
//! and optional network namespace separation.
//!
//! # Example
//!
//! ```no_run
//! # async fn doc() {
//! use avid_sandbox::{Sandbox, SandboxConfig};
//! use std::collections::HashMap;
//!
//! let sandbox = Sandbox::new(SandboxConfig::default());
//! let files = HashMap::from([("main.py".into(), "print(42)".into())]);
//! let result = sandbox.run(&files, "main.py").await.expect("execution");
//! assert_eq!(result.stdout.trim(), "42");
//! # }
//! ```

mod config;
mod error;
mod limits;
mod runner;
#[cfg(test)]
mod tests;

pub use config::{ExecStatus, SandboxConfig};
pub use error::SandboxError;
pub use runner::{ExecutionResult, Sandbox};

use std::collections::HashMap;

impl Sandbox {
    /// Create a new sandbox with the given configuration.
    #[must_use]
    pub fn new(cfg: SandboxConfig) -> Self {
        let use_netns = cfg.use_netns && which::which("unshare").is_ok();
        let config = SandboxConfig { use_netns, ..cfg };
        Self { config }
    }

    /// Run Python files in the sandbox.
    ///
    /// # Errors
    /// Returns `SandboxError` on spawn failure, I/O errors, or sandbox violations.
    pub async fn run(
        &self,
        files: &HashMap<String, String>,
        entrypoint: &str,
    ) -> Result<ExecutionResult, SandboxError> {
        runner::run_sandboxed(&self.config, files, entrypoint).await
    }
}
