//! TurboQuant Proxy — Live KV cache compression bridge between llama-server and 3-bit storage.
//!
//! Architecture:
//! ```text
//! Client → TurboQuantProxy (:8082) → llama-server (:8081)
//!                    ↓
//!         TurboQuantKVCache (3-bit compressed offload)
//!                    ↓
//!         When GPU KV full → evict old positions → compress to 3-bit
//!         When need old context → decompress from 3-bit → inject back
//! ```
//!
//! Effective compression: Q4_0 (4x) + TurboQuant 3-bit offload = 6x total vs FP16.

pub mod server;
pub mod kv_bridge;
pub mod metrics;
pub mod router;

pub use server::TurboQuantProxy;
pub use kv_bridge::KVBridge;
pub use metrics::ProxyMetrics;