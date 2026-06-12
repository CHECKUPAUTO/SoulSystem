use thiserror::Error;

/// Errors produced by the [`AntiClone`](crate) engine.
#[derive(Debug, Error)]
pub enum AntiCloneError {
    #[error("failed to parse Python source: {0}")]
    Parse(String),
}
