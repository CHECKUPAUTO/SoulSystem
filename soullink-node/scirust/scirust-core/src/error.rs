//! Unified error type for all SciRust crates.

use std::fmt;

/// Unified error type across all SciRust crates.
#[derive(Debug, Clone)]
pub enum SciError {
    /// Parse error (expression, tokens, etc.)
    Parse(String),
    /// Shape mismatch for tensor operations
    Shape { expected: Vec<usize>, got: Vec<usize> },
    /// Singular or ill-conditioned matrix
    SingularMatrix(String),
    /// Contradiction detected in consistency engine
    Contradiction(Vec<String>),
    /// Node or layer not found
    NotFound(String),
    /// Dimension or value out of range
    OutOfRange(String),
    /// I/O error (serialization, etc.)
    Io(String),
    /// Generic computation error
    Compute(String),
}

impl fmt::Display for SciError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SciError::Parse(msg) => write!(f, "Parse error: {}", msg),
            SciError::Shape { expected, got } => {
                write!(f, "Shape mismatch: expected {:?}, got {:?}", expected, got)
            }
            SciError::SingularMatrix(msg) => write!(f, "Singular matrix: {}", msg),
            SciError::Contradiction(issues) => {
                write!(f, "Contradiction ({} issues): ", issues.len())?;
                for (i, issue) in issues.iter().enumerate() {
                    if i > 0 { write!(f, "; ")?; }
                    write!(f, "{}", issue)?;
                }
                Ok(())
            }
            SciError::NotFound(msg) => write!(f, "Not found: {}", msg),
            SciError::OutOfRange(msg) => write!(f, "Out of range: {}", msg),
            SciError::Io(msg) => write!(f, "I/O error: {}", msg),
            SciError::Compute(msg) => write!(f, "Compute error: {}", msg),
        }
    }
}

impl std::error::Error for SciError {}

/// Convenience result type for SciRust operations.
pub type CoreResult<T> = Result<T, SciError>;

/// Convert from String errors to SciError::Compute.
impl From<String> for SciError {
    fn from(s: String) -> Self {
        SciError::Compute(s)
    }
}

impl From<&str> for SciError {
    fn from(s: &str) -> Self {
        SciError::Compute(s.to_string())
    }
}
