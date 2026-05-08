//! soullink-trt — TensorRT-LLM bindings for direct GPU inference.
//!
//! Two modes:
//! - **Stub** (default): Compiles without TRT-LLM libraries. All generate calls
//!   return `TrtError::FeatureDisabled`, triggering Ollama fallback.
//! - **trt-llm** feature: Full FFI via cxx. Direct GPU inference bypassing HTTP.
//!
//! Architecture: Model load → create session → generate/generate_stream → drop session.

pub mod types;
pub mod engine;
pub mod batch;

#[cfg(feature = "trt-llm")]
pub mod ffi;

pub use types::{TrtConfig, TrtError, GenerationConfig, GenerationResult, StreamChunk, BatchItem};
pub use engine::TrtEngine;
pub use batch::BatchScheduler;