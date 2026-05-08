//! TurboQuant 3-bit KV Cache Compression
//! 
//! Google Research (March 2026) — PolarQuant + QJL algorithm.
//! Reduces KV cache memory by ~6x vs FP16 while maintaining <0.1% quality loss.
//!
//! # Architecture
//! ```text
//! FP16 K/V → PolarQuant (random orthogonal rotation) → QJL (3-bit quant + 1-bit correction) → Packed bytes
//! ```
//!
//! # Integration with llama.cpp
//! The llama-server with `--cache-type-k q4_0` gives 4x compression baseline.
//! Our TurboQuant layer sits on top and compresses the *evicted* KV pages
//! from 4-bit to 3-bit, achieving effective 6x vs FP16.

pub mod polar;
pub mod qjl;
pub mod cache;
pub mod attention;
pub mod proxy;

pub use cache::TurboQuantKVCache;
pub use polar::PolarQuant;
pub use qjl::QJLQuantizer;
pub use attention::TurboQuantAttention;
