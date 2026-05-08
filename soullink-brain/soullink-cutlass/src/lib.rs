//! soullink-cutlass — CUTLASS GPU kernel bindings for SoulLink.
//!
//! Stub mode by default. Enable `cutlass` feature for real CUDA FFI.

pub mod types;
pub mod memory;
pub mod stream;
pub mod kernel;

pub use types::{CutlassError, KernelConfig};
pub use memory::GpuTensor;
pub use stream::CudaStream;
pub use kernel::{cosine_sim_batch, topk, hnn_verlet_step};