//! Hamiltonian Neural Network — learns energy surfaces via Dual-number autodiff.
//!
//! This crate depends on `scirust-autodiff`, which is an external crate
//! not yet on crates.io. To avoid blocking the whole workspace build
//! when that path is unavailable, the actual implementation is gated
//! behind the `autodiff` feature.
//!
//! Without the feature: this crate compiles to an empty shell — useful
//! for keeping `cargo check --workspace` green across all environments.
//! Enable the feature to build the HNN:
//!
//! ```bash
//! cargo build -p avid-hnn --features autodiff
//! ```

#[cfg(feature = "autodiff")]
pub mod hnn;

/// Stub re-export so downstream code can `use avid_hnn::IS_ENABLED;` to
/// detect whether the real implementation is compiled in.
pub const IS_ENABLED: bool = cfg!(feature = "autodiff");
