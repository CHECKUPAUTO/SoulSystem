//! soullink-cutlass — CUTLASS GPU kernel bindings for SoulLink.
//!
//! Stub mode by default. Enable `cutlass` feature for real CUDA FFI.

pub mod kernel;
pub mod memory;
pub mod stream;
pub mod types;

pub use kernel::{cosine_sim_batch, hnn_verlet_step, topk};
pub use memory::GpuTensor;
pub use stream::CudaStream;
pub use types::{CutlassError, KernelConfig};
