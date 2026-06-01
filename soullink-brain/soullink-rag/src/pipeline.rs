//! SoulFlow Pipeline — end-to-end document ingestion to long-term memory.
//!
//! Flow: Document → Ollama Embed → CUTLASS Dedup → Sled/HNSW → MemoryGraph
//!
//! Each step respects VRAM-awareness via ModelRouter thresholds.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{debug, info, warn};
use uuid::Uuid;

/// Unique document identifier.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct DocumentId(pub String);

impl DocumentId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }
}

/// Metadata for an ingested document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentMeta {
    pub id: DocumentId,
    pub source_path: String,
    pub title: String,
    pub chunk_count: usize,
    pub total_chars: usize,
    pub ingested_at: u64,
}

/// Result of a full ingestion pipeline run.
#[derive(Debug, Serialize, Deserialize)]
pub struct IngestResult {
    pub document_id: DocumentId,
    pub chunks_ingested: usize,
    pub chunks_skipped_dedup: usize,
    pub elapsed_ms: u64,
    pub status: IngestStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IngestStatus {
    /// All chunks ingested successfully.
    Complete,
    /// Some chunks skipped as duplicates.
    PartialDedup,
    /// Document is entirely duplicate.
    Duplicate,
    /// VRAM insufficient — deferred until Ollama purge.
    VramWait,
}

/// Configuration for the SoulFlow pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    /// Ollama embedding model name.
    pub embed_model: String,
    /// Embedding dimension (768 for nomic-embed-text).
    pub embed_dim: usize,
    /// Cosine similarity threshold for dedup (above = duplicate).
    pub dedup_threshold: f32,
    /// Maximum chunks per document.
    pub max_chunks: usize,
    /// Chunk size in characters.
    pub chunk_size: usize,
    /// Chunk overlap in characters.
    pub chunk_overlap: usize,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            embed_model: "nomic-embed-text".to_string(),
            embed_dim: 768,
            dedup_threshold: 0.98,
            max_chunks: 500,
            chunk_size: 512,
            chunk_overlap: 64,
        }
    }
}

/// A search hit from the knowledge base.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub chunk_id: String,
    pub text: String,
    pub source: String,
    pub score: f32,
}

/// Text chunker — splits documents into overlapping chunks.
/// Separate from the full pipeline so it can be tested without GPU/store.
pub struct TextChunker {
    pub chunk_size: usize,
    pub chunk_overlap: usize,
    pub max_chunks: usize,
}

impl TextChunker {
    pub fn new(config: &PipelineConfig) -> Self {
        Self {
            chunk_size: config.chunk_size,
            chunk_overlap: config.chunk_overlap,
            max_chunks: config.max_chunks,
        }
    }

    /// Split text into overlapping chunks, breaking at word boundaries.
    pub fn chunk(&self, text: &str) -> Vec<String> {
        let mut chunks = Vec::new();
        let chars: Vec<char> = text.chars().collect();
        let mut start = 0;

        while start < chars.len() && chunks.len() < self.max_chunks {
            let end = std::cmp::min(start + self.chunk_size, chars.len());

            // Try to break at word boundary (look for space)
            let mut actual_end = end;
            if end < chars.len() {
                for i in ((start + self.chunk_size / 2)..end).rev() {
                    if chars[i] == ' ' {
                        actual_end = i;
                        break;
                    }
                }
            }

            let chunk: String = chars[start..actual_end].iter().collect();
            let trimmed = chunk.trim().to_string();
            if !trimmed.is_empty() {
                chunks.push(trimmed);
            }

            start = if actual_end == end {
                end
            } else {
                actual_end + 1
            };

            // Apply overlap
            if start >= self.chunk_overlap && start < chars.len() {
                start -= self.chunk_overlap;
            }
        }

        chunks
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_text_basic() {
        let config = PipelineConfig {
            chunk_size: 50,
            chunk_overlap: 10,
            ..Default::default()
        };
        let chunker = TextChunker::new(&config);
        let text = "Hello world this is a test of chunking with some words and more words to fill up space.";
        let chunks = chunker.chunk(text);
        assert!(!chunks.is_empty(), "Should produce at least one chunk");
        assert!(chunks.len() <= 10, "Should not produce excessive chunks");
    }

    #[test]
    fn chunk_text_empty() {
        let config = PipelineConfig::default();
        let chunker = TextChunker::new(&config);
        let chunks = chunker.chunk("");
        assert!(chunks.is_empty());
    }

    #[test]
    fn chunk_text_respects_max() {
        let config = PipelineConfig {
            max_chunks: 3,
            chunk_size: 10,
            chunk_overlap: 0,
            ..Default::default()
        };
        let chunker = TextChunker::new(&config);
        let text = "a b c d e f g h i j k l m n o p";
        let chunks = chunker.chunk(text);
        assert!(chunks.len() <= 3, "Should respect max_chunks");
    }

    #[test]
    fn document_id_uniqueness() {
        let id1 = DocumentId::new();
        let id2 = DocumentId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn pipeline_config_defaults() {
        let config = PipelineConfig::default();
        assert_eq!(config.embed_model, "nomic-embed-text");
        assert_eq!(config.embed_dim, 768);
        assert_eq!(config.dedup_threshold, 0.98);
        assert_eq!(config.chunk_size, 512);
    }

    #[test]
    fn ingest_status_serialization() {
        let status = IngestStatus::PartialDedup;
        let json = serde_json::to_string(&status).unwrap();
        let back: IngestStatus = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, IngestStatus::PartialDedup));
    }
}
