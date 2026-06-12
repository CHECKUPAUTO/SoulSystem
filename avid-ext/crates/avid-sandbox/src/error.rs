use thiserror::Error;

/// Errors produced by the sandbox engine.
#[derive(Debug, Error)]
pub enum SandboxError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("spawn error: {0}")]
    Spawn(String),

    #[error("invalid configuration: {0}")]
    Config(String),

    #[error("timeout")]
    Timeout,
}
