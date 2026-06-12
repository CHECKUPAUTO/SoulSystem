//! CXX bridge for TensorRT-LLM FFI.
//!
//! This module is only compiled when the `trt-llm` feature is enabled.
//! It provides the Rust side of the cxx bridge. The C++ side is in
//! `cxx-bridge/trt_llm.cpp`.

#[cfg(feature = "trt-llm")]
use cxx;

#[cfg(feature = "trt-llm")]
#[cxx::bridge]
pub mod ffi {
    extern "C++" {
        // Opaque C++ runtime wrapper — owned by Rust via UniquePtr
        type TllmRuntimeWrapper;

        /// Load a TensorRT engine from disk.
        /// Returns a new TllmRuntimeWrapper on success.
        fn load_engine(path: &str, max_batch_size: u32) -> Result<UniquePtr<TllmRuntimeWrapper>>;

        /// Generate text synchronously.
        /// Returns the generated string.
        fn generate(
            runtime: &TllmRuntimeWrapper,
            prompt: &str,
            max_tokens: u32,
            temperature: f32,
        ) -> Result<String>;

        /// Get VRAM usage in MiB.
        fn vram_used_mb(runtime: &TllmRuntimeWrapper) -> u64;

        /// Get total VRAM in MiB.
        fn vram_total_mb(runtime: &TllmRuntimeWrapper) -> u64;
    }
}

// In stub mode (no feature), we provide a stub load_engine
// that the engine module never calls (it returns FeatureDisabled first).
#[cfg(not(feature = "trt-llm"))]
pub fn load_engine(_path: &str, _max_batch_size: u32) -> Result<(), crate::types::TrtError> {
    Err(crate::types::TrtError::FeatureDisabled)
}
