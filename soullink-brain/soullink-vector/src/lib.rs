//! SoulLink Vector — SIMD-accelerated vector search with dimension safety.
//!
//! Modules:
//! - `alignment`: DimensionRegistry (CAS-locked dimension + padding)
//! - `hnsw_index`: HNSW + HybridSearch + VectorStore (hnsw_rs 0.3, AVX2)
//! - `channel`: async-channel benchmark vs tokio::mpsc

pub mod alignment;
pub mod channel;
pub mod error;
pub mod hnsw_index;

pub use hnsw_index::{HnswIndex, HnswParams, HybridSearch, SearchResult, VectorError, VectorStore};