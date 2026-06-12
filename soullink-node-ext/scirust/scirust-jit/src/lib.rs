//! SciRust JIT — Runtime kernel compilation and dispatch.
//!
//! Generates optimized Rust source code for compute kernels (GEMM, activations),
//! compiles them with `rustc` at runtime, and caches the resulting shared
//! libraries for subsequent calls.
//!
//! Falls back gracefully to the scalar implementations if `rustc` is unavailable
//! or compilation fails.
//!
//! # Architecture
//!
//! ```text
//! Request → KernelTemplate (source gen) → rustc compile → dlopen → execute
//!                                ↓                            ↓
//!                           Cache miss                    Cache hit
//! ```

#![allow(unused)]

pub mod kernel;
pub mod cache;
pub mod dispatch;

pub mod prelude {
    pub use crate::kernel::*;
    pub use crate::cache::*;
    pub use crate::dispatch::*;
}
