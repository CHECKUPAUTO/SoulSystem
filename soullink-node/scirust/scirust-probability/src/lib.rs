//! SciRust Probability — Probabilistic inference engine.
//!
//! Combines Bayesian networks, consistency checking, hypothesis verification,
//! and inference memory for a complete scientific reasoning system.
//!
//! # Modules
//!
//! - `models` — Prior/posterior probability distributions
//! - `bayesian` — BayesianNode, BayesianNetwork, inference (exact + approximate)
//! - `consistency` — Logical rules, contradiction detection, consistency scoring
//! - `hypothesis` — Hypothesis verification loop (LLM hybrid interface)
//! - `memory` — Inference step tracing, path reconstruction, A* search
//!
//! # Integration
//!
//! This crate integrates with:
//! - `scirust-symbolic` for symbolic expression representation
//! - `scirust-reasoning` for equation solving and proof checking

#![allow(unused)]

pub mod models;
pub mod bayesian;
pub mod consistency;
pub mod hypothesis;
pub mod memory;

pub mod prelude {
    pub use crate::models::*;
    pub use crate::bayesian::*;
    pub use crate::consistency::*;
    pub use crate::hypothesis::*;
    pub use crate::memory::*;
}
