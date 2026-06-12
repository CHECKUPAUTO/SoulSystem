use thiserror::Error;

#[derive(Error, Debug)]
pub enum VectorError {
    #[error("dimension mismatch: expected {expected}, got {got}")]
    DimensionMismatch { expected: usize, got: usize },

    #[error("device unavailable: {0}")]
    DeviceUnavailable(String),

    #[error("graph poisoned: {0}")]
    GraphPoisoned(String),

    #[error("channel closed")]
    ChannelClosed,
}

pub type Result<T> = std::result::Result<T, VectorError>;
