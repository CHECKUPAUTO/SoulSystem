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
//! SoulLink Senate — Multi-expert deliberation via parallel Ollama calls.
//!
//! Inspired by "Mixture of Experts" and "Debate" patterns.
//! Multiple Ollama models answer the same prompt; a Critic aggregates
//! their responses into a final consensus.

pub mod aggregator;
pub mod senate;

pub use aggregator::{AggregatedResult, AggregationStrategy};
pub use senate::Senate;
