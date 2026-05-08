//! soullink-rag — Page-aware RAG with hybrid search (HNSW + BM25 + Sled).
//!
//! SoulFlow pipeline: Document → Embed → Dedup → Store → Search

pub mod bm25;
pub mod embedding;
pub mod page;
pub mod pipeline;
pub mod provider;
pub mod store;

pub use pipeline::{PipelineConfig, IngestResult, IngestStatus, SearchHit, DocumentId, DocumentMeta, TextChunker};
pub use store::PageAwareStore;
pub use embedding::{EmbedBridge, EmbedConfig};