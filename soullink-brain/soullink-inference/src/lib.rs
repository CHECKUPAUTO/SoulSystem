//! soullink-inference — NUMA-aware inference engine for SoulLink V13.5
//!
//! Optimizes LLM inference routing across dual Xeon NUMA + RTX 4060:
//! - GPU (Think, low latency) vs CPU NUMA-local (Dream, throughput)
//! - Dynamic quantization (Q4_K_M ↔ Q2_K) based on cycle
//! - Priority batching: Think > Dream > Embed
//! - Warm standby for preloaded models

pub mod hardware;
pub mod router;
pub mod batch;
pub mod standby;
pub mod engine;
pub mod types;
pub mod turboquant;

pub use engine::InferenceEngine;
pub use types::*;
pub use turboquant::TurboQuantKVCache;