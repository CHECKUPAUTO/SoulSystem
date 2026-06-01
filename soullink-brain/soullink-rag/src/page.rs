//! Page chunk and metadata types for page-aware RAG.

use serde::{Deserialize, Serialize};

/// Type of content in a chunk.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ChunkType {
    /// Main body text.
    Body,
    /// Header or title section.
    Header,
    /// Footer, page number, copyright notice.
    Footer,
    /// Table data.
    Table,
    /// Code listing.
    Code,
    /// Image caption or alt text.
    Caption,
}

/// Metadata attached to each page chunk in the index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageMetadata {
    /// UUID of the source document.
    pub doc_id: String,
    /// First page number in this chunk (0-indexed).
    pub start_page: usize,
    /// Last page number in this chunk (exclusive).
    pub end_page: usize,
    /// Type of content.
    pub chunk_type: ChunkType,
    /// Section title if detectable.
    pub section: Option<String>,
    /// First 200 chars for preview/scoring.
    pub text_preview: String,
}

/// A page chunk ready for indexing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageChunk {
    /// Unique identifier for this chunk.
    pub id: String,
    /// The full text content of this chunk.
    pub text: String,
    /// Metadata about source and position.
    pub metadata: PageMetadata,
}

impl PageChunk {
    /// Create a new chunk with auto-generated ID.
    pub fn new(
        doc_id: &str,
        start_page: usize,
        end_page: usize,
        text: String,
        chunk_type: ChunkType,
    ) -> Self {
        let id = format!("{}:p{}-{}", doc_id, start_page, end_page);
        let text_preview: String = text.chars().take(200).collect();
        Self {
            id,
            text,
            metadata: PageMetadata {
                doc_id: doc_id.to_string(),
                start_page,
                end_page,
                chunk_type,
                section: None,
                text_preview,
            },
        }
    }
}

/// Configuration for sliding window ingestion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageConfig {
    /// Number of pages per chunk window.
    pub window_size: usize,
    /// Overlap between consecutive windows (in pages).
    pub overlap: usize,
    /// Minimum chunk length in characters (skip near-empty chunks).
    pub min_chunk_chars: usize,
    /// Maximum chunk length in characters (split longer).
    pub max_chunk_chars: usize,
    /// Whether to detect and filter headers/footers.
    pub clean_headers_footers: bool,
}

impl Default for PageConfig {
    fn default() -> Self {
        Self {
            window_size: 2,
            overlap: 1,
            min_chunk_chars: 50,
            max_chunk_chars: 8000,
            clean_headers_footers: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_chunk_creation() {
        let chunk = PageChunk::new("doc-123", 0, 2, "Hello world".to_string(), ChunkType::Body);
        assert_eq!(chunk.id, "doc-123:p0-2");
        assert_eq!(chunk.metadata.start_page, 0);
        assert_eq!(chunk.metadata.end_page, 2);
        assert_eq!(chunk.metadata.chunk_type, ChunkType::Body);
    }

    #[test]
    fn text_preview_truncation() {
        let long_text = "x".repeat(500);
        let chunk = PageChunk::new("doc", 0, 1, long_text, ChunkType::Body);
        assert_eq!(chunk.metadata.text_preview.len(), 200);
    }

    #[test]
    fn page_config_defaults() {
        let config = PageConfig::default();
        assert_eq!(config.window_size, 2);
        assert_eq!(config.overlap, 1);
        assert!(config.clean_headers_footers);
    }
}
