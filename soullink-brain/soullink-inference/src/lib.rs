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
//! soullink-inference — NUMA-aware inference engine for SoulLink V13.5
//!
//! Optimizes LLM inference routing across dual Xeon NUMA + RTX 4060:
//! - GPU (Think, low latency) vs CPU NUMA-local (Dream, throughput)
//! - Dynamic quantization (Q4_K_M ↔ Q2_K) based on cycle
//! - Priority batching: Think > Dream > Embed
//! - Warm standby for preloaded models

pub mod batch;
pub mod engine;
pub mod hardware;
pub mod mtp;
pub mod router;
pub mod standby;
pub mod turboquant;
pub mod types;

pub use engine::InferenceEngine;
pub use turboquant::TurboQuantKVCache;
pub use types::*;
