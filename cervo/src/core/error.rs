use thiserror::Error;

#[derive(Debug, Clone, Error)]
pub enum TransformationError {
    #[error("Invalid operation: {0}")]
    InvalidOperation(String),
    #[error("Overflow: {0}")]
    Overflow(String),
}

#[derive(Debug, Clone, Error)]
pub enum StabilityError {
    #[error("Infinite loop detected during sandbox test")]
    InfiniteLoop,
    #[error("Memory overflow: output size {0} bytes exceeds limit")]
    MemoryOverflow(usize),
    #[error("Performance degradation: execution took {0:?}")]
    PerformanceDegradation(std::time::Duration),
    #[error("Excessive divergence: {0:.2}")]
    ExcessiveDivergence(f64),
    #[error("Transformation error: {0}")]
    TransformationError(#[from] TransformationError),
}
