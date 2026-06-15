//! SciRust Core — scientific computing engine for SoulLink ecosystem.
//!
//! # Modules
//! - **autodiff** — reverse-mode (Tensor, Tape) and forward-mode (Dual) automatic differentiation
//! - **matrix** — SIMD backend trait, matrix views with zero-copy slicing
//! - **embed** — text embedding engine (random-projection LSH)
//! - **symbolic** — expression parsing, simplification, evaluation, proving
//! - **learning** — polynomial fitting, pattern discovery
//! - **nn** — neural network building blocks (mini LLM, transformers)
//! - **solve** — linear/quadratic solvers, optimizer
//! - **pattern** — pattern memory for experience replay

pub mod autodiff;
pub mod dispatch;
pub mod embed;
pub mod learning;
pub mod matrix;
pub mod nn;
pub mod pattern;
pub mod simd;
pub mod solve;
pub mod symbolic;

// ── Re-exports at crate root for ergonomic access ───────────────────────────

pub use autodiff::forward::Dual;
pub use autodiff::reverse::{Tape, Tensor};
pub use embed::EmbeddingEngine;
pub use learning::{discover_patterns, polynomial_fit};
pub use pattern::PatternMemory;
pub use simd::simd_add_one;
pub use solve::{solve_linear, solve_quadratic, Optimizer};
pub use symbolic::{eval, parse, prove_equal, simplify, Expr};

/// Items commonly used together.
pub mod prelude {
    pub use crate::autodiff::forward::Dual;
    pub use crate::autodiff::reverse::{Tape, Tensor};
    pub use crate::embed::EmbeddingEngine;
    pub use crate::learning::{discover_patterns, linear_regression, polynomial_fit};
    pub use crate::pattern::PatternMemory;
    pub use crate::simd::{add_f32, add_f64, mul_f32, mul_f64};
    pub use crate::solve::{solve_linear, solve_quadratic, Optimizer};
    pub use crate::symbolic::{diff, eval, parse, parse_natural, prove_equal, simplify, Expr};

    #[cfg(feature = "gpu")]
    pub use crate::dispatch::gpu_or_cpu;
}
