//! soullink-dali — NVIDIA DALI bindings for GPU-accelerated data preprocessing.
//!
//! Stub mode by default. Enable `dali` feature for real FFI.

mod pipeline;
mod types;

pub use pipeline::{DaliPipeline, DaliPipelineBuilder};
pub use types::{DaliError, DaliTensor, Device};
