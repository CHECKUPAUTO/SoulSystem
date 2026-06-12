#![allow(
    dead_code,
    unused_imports,
    unused_variables,
    unused_mut,
    unused_unsafe,
    unreachable_pub,
    non_camel_case_types,
    non_snake_case,
    unused_comparisons
)]
//! soullink-dali — NVIDIA DALI bindings for GPU-accelerated data preprocessing.
//!
//! Stub mode by default. Enable `dali` feature for real FFI.

mod pipeline;
mod types;

pub use pipeline::{DaliPipeline, DaliPipelineBuilder};
pub use types::{DaliError, DaliTensor, Device};
