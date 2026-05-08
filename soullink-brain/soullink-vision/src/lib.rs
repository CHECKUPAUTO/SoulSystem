//! SoulLink Vision — Image embedding pipeline for RTX 4060.
//!
//! Provides CLIP/Moondream embedding generation via Ollama.
//! Images are base64-encoded and sent to the vision model for embedding extraction.

pub mod pipeline;
pub mod embedding;

pub use pipeline::VisionPipeline;
pub use embedding::{ImageEmbedding, EmbeddingBackend};