//! CUTLASS error types and kernel configuration.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CutlassError {
    #[error("CUDA error: {0}")]
    Cuda(String),
    #[error("Kernel execution failed: {0}")]
    Kernel(String),
    #[error("Memory transfer failed: {0}")]
    Memory(String),
    #[error("Feature 'cutlass' is disabled. Recompile with --features cutlass")]
    FeatureDisabled,
}

#[cfg(feature = "cutlass")]
impl From<cust::error::CudaError> for CutlassError {
    fn from(err: cust::error::CudaError) -> Self {
        CutlassError::Cuda(err.to_string())
    }
}

#[derive(Debug, Clone)]
pub struct KernelConfig {
    pub grid_size: u32,
    pub block_size: u32,
    pub shared_mem_bytes: u32,
}

impl Default for KernelConfig {
    fn default() -> Self {
        Self {
            grid_size: 1,
            block_size: 256,
            shared_mem_bytes: 0,
        }
    }
}