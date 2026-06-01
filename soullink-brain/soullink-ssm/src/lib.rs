//! Soullink-SSM: Mamba-style State Space Models in Rust.
//!
//! Implements selective state space models (S6 / Mamba) with
//! HiPPO initialization, parallel scan, and hardware-aware kernels.
//!
//! # Architecture
//!
//! - **HiPPO:** Optimal polynomial projection initialization for A matrix
//! - **SSM:** Core continuous-time state space model with ZOH discretization
//! - **SelectiveScan:** Input-dependent parameterization (Δ, B, C)
//! - **ParallelScan:** Associative scan for O(L log L) training
//! - **MambaBlock:** Full gated SSM layer with conv + SiLU + residual
//! - **MambaModel:** Stacked Mamba blocks with embedding/projection heads

pub mod cuda;
pub mod hippo;
pub mod layer;
pub mod model;
pub mod parallel;
pub mod quantization;
pub mod selective;
pub mod ssm;
pub mod tiled_scan;

// Re-export the public API
pub use hippo::HiPPOInitializer;
pub use layer::{GateType, MambaBlock, MambaConfig};
pub use model::MambaModel;
pub use parallel::{MultiDimScan, ParallelScan};
pub use quantization::QuantizedMatrix;
pub use selective::{MultiDimSelectiveSSM, SelectiveSSM};
pub use ssm::{discretize_zoh, matrix_exp, SSMConfig, SSMKernel};
pub use tiled_scan::TiledScan;
// new function
