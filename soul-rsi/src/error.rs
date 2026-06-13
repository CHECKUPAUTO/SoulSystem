//! Error type for the recursive self-improvement loop.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum RsiError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("the patch target {0} is outside the allowed paths")]
    PathNotAllowed(String),

    #[error("could not apply patch: {0}")]
    Apply(String),

    #[error("evaluation failed: {0}")]
    Evaluation(String),

    #[error("proposer failed: {0}")]
    Proposer(String),

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, RsiError>;
