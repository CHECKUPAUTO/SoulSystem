//! SoulLink Senate — Multi-expert deliberation via parallel Ollama calls.
//!
//! Inspired by "Mixture of Experts" and "Debate" patterns.
//! Multiple Ollama models answer the same prompt; a Critic aggregates
//! their responses into a final consensus.

pub mod senate;
pub mod aggregator;

pub use senate::Senate;
pub use aggregator::{AggregationStrategy, AggregatedResult};