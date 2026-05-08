//! soullink-dali — NVIDIA DALI bindings for GPU-accelerated data preprocessing.
//!
//! Stub mode by default. Enable `dali` feature for real FFI.

mod types;
mod pipeline;

pub use types::{DaliError, Device, DaliTensor};
pub use pipeline::{DaliPipeline, DaliPipelineBuilder};