//! DALI types and error definitions.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DaliError {
    #[error("DALI initialization failed: {0}")]
    Init(String),
    #[error("Pipeline build error: {0}")]
    Build(String),
    #[error("Runtime execution error: {0}")]
    Run(String),
    #[error("GPU Out of Memory")]
    OutOfMemory,
    #[error("DALI feature is disabled. Compile with --features dali")]
    FeatureDisabled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Device {
    Cpu,
    Gpu(i32),
    Mixed,
}

/// A tensor produced by a DALI pipeline. Lives on GPU by default.
pub struct DaliTensor {
    pub(crate) shape: Vec<usize>,
    pub(crate) device: Device,
}

impl DaliTensor {
    pub fn shape(&self) -> &[usize] {
        &self.shape
    }

    pub fn device(&self) -> &Device {
        &self.device
    }

    /// Explicit GPU → CPU copy.
    pub fn to_vec(&self) -> Result<Vec<f32>, DaliError> {
        #[cfg(not(feature = "dali"))]
        return Err(DaliError::FeatureDisabled);

        #[cfg(feature = "dali")]
        {
            // FFI call to cudaMemcpy via the C++ bridge
            todo!("implement via dali_wrapper::copy_output_to_host")
        }
    }
}