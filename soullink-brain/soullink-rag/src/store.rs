//! PageAwareStore — the core RAG engine combining HNSW vector search + BM25 keyword search.
//!
//! Ingests documents via PageProvider with sliding window, vectorizes chunks
//! through EmbedBridge (Ollama async), stores vectors in HNSW, text in BM25,
//! metadata in Sled. Supports hybrid search (vector × keyword via RRF).

use crate::bm25::Bm25Index;
use crate::embedding::{EmbedBridge, EmbedConfig};
use crate::page::{ChunkType, PageChunk, PageConfig, PageMetadata};
use crate::provider::PageProvider;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use soullink_vector::hnsw_index::{HnswIndex, HnswParams, SearchResult};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, instrument, warn};

/// A search hit combining vector similarity and keyword relevance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    /// Chunk ID (doc_id:pX-Y).
    pub id: String,
    /// Document ID.
    pub doc_id: String,
    /// Page range (start, end exclusive).
    pub page_range: (usize, usize),
    /// Vector cosine similarity score [0, 1].
    pub vector_score: f32,
    /// BM25 keyword score (0 if no keyword match).
    pub keyword_score: f64,
    /// Combined hybrid score.
    pub hybrid_score: f64,
    /// Chunk type.
    pub chunk_type: ChunkType,
    /// Text preview (first 200 chars).
    pub text_preview: String,
}

/// Result of a hybrid search.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridResult {
    pub hits: Vec<SearchHit>,
    pub total_vector_results: usize,
    pub total_keyword_results: usize,
    pub query: String,
}

/// The page-aware RAG store — combines HNSW vectors + BM25 keywords + Sled metadata.
///
/// Async ingestion via EmbedBridge (Ollama). Falls back to placeholder vectors
/// if Ollama is unavailable.
pub struct PageAwareStore {
    /// HNSW vector index for semantic search (behind Mutex for async mutation).
    vector_index: Arc<HnswIndex>,
    /// BM25 index for keyword search (behind Mutex for async mutation).
    keyword_index: Arc<Mutex<Bm25Index>>,
    /// Sled database for metadata persistence.
    meta_db: sled::Db,
    /// Embedding bridge (Ollama async).
    embed_bridge: EmbedBridge,
    /// Sliding window configuration.
    config: PageConfig,
    /// Dimension of embedding vectors.
    dim: usize,
}

impl PageAwareStore {
    /// Open or create a PageAwareStore at the given path.
    pub fn open(
        path: &Path,
        dim: usize,
        config: PageConfig,
        embed_config: EmbedConfig,
    ) -> Result<Self> {
        let vector_index = HnswIndex::new(dim, HnswParams::default(), 10_000);
        let keyword_index = Bm25Index::new();
        let meta_db = sled::open(path.join("metadata"))?;
        let embed_bridge = EmbedBridge::new(embed_config);

        Ok(Self {
            vector_index,
            keyword_index: Arc::new(Mutex::new(keyword_index)),
            meta_db,
            embed_bridge,
            config,
            dim,
        })
    }

    /// Open with default embedding config (nomic-embed-text, 768-dim).
    pub fn open_default(path: &Path, dim: usize, config: PageConfig) -> Result<Self> {
        Self::open(path, dim, config, EmbedConfig::default())
    }

    /// Ingest a document using a PageProvider with sliding window.
    ///
    /// For each window:
    /// 1. Aggregate pages → text
    /// 2. Clean content (headers/footers)
    /// 3. Detect chunk type
    /// 4. Vectorize via EmbedBridge (async, with semaphore)
    /// 5. Insert into HNSW + BM25 + Sled
    #[instrument(skip(self, provider), fields(doc = %provider.document().id))]
    pub async fn ingest<P: PageProvider>(&self, provider: &P) -> Result<usize> {
        let total_pages = provider.get_page_count();
        if total_pages == 0 {
            return Ok(0);
        }

        let step = self
            .config
            .window_size
            .saturating_sub(self.config.overlap)
            .max(1);
        let mut chunks_ingested = 0;

        // Phase 1: Generate chunks
        let mut chunks: Vec<PageChunk> = Vec::new();
        for start in (0..total_pages).step_by(step) {
            let end = std::cmp::min(start + self.config.window_size, total_pages);

            let mut window_text = String::new();
            for p in start..end {
                if let Some(content) = provider.get_page_content(p) {
                    let cleaned = provider.clean_content(content);
                    if !cleaned.is_empty() {
                        window_text.push_str(&cleaned);
                        window_text.push('\n');
                    }
                }
            }

            if window_text.len() < self.config.min_chunk_chars {
                continue;
            }
            if window_text.len() > self.config.max_chunk_chars {
                window_text = window_text
                    .chars()
                    .take(self.config.max_chunk_chars)
                    .collect();
            }

            let chunk_type = provider.detect_chunk_type(&window_text);
            let chunk =
                PageChunk::new(&provider.document().id, start, end, window_text, chunk_type);
            chunks.push(chunk);
        }

        info!(
            "Ingesting {} chunks from doc '{}'",
            chunks.len(),
            provider.document().id
        );

        // Phase 2: Vectorize all chunks in parallel via EmbedBridge
        let texts: Vec<String> = chunks.iter().map(|c| c.text.clone()).collect();
        info!("Vectorizing {} chunks via EmbedBridge", texts.len());
        let embeddings = self.embed_bridge.embed_batch(&texts).await;

        // Phase 3: Store everything
        for (i, chunk) in chunks.iter().enumerate() {
            // Get embedding (fallback to placeholder if Ollama failed)
            let vector = match &embeddings[i] {
                Ok(emb) => emb.clone(),
                Err(e) => {
                    warn!(
                        "Embedding failed for chunk {}: {}, using placeholder",
                        chunk.id, e
                    );
                    self.placeholder_embedding(&chunk.text)
                }
            };

            // Validate dimension
            if vector.len() != self.dim {
                warn!(
                    "Dimension mismatch for chunk {}: expected {}, got {} — skipping",
                    chunk.id,
                    self.dim,
                    vector.len()
                );
                continue;
            }

            // Store metadata in Sled
            let meta_key = chunk.id.as_bytes();
            let meta_val = serde_json::to_vec(&chunk.metadata)?;
            self.meta_db.insert(meta_key, meta_val)?;

            // Insert into BM25 keyword index
            {
                let mut bm25 = self.keyword_index.lock().await;
                bm25.add(&chunk.id, &chunk.text);
            }

            // Insert into HNSW vector index
            self.vector_index.insert(chunk.id.clone(), &vector)?;

            chunks_ingested += 1;
        }

        // Flush metadata
        self.meta_db.flush()?;

        // Seal HNSW for optimized search after bulk ingestion
        self.vector_index.seal_for_search();

        info!(
            "Ingested {} chunks from doc '{}'",
            chunks_ingested,
            provider.document().id
        );
        Ok(chunks_ingested)
    }

    /// Hybrid search: combines vector similarity and BM25 keyword matching.
    ///
    /// alpha controls the balance:
    /// - alpha = 1.0 → pure vector search
    /// - alpha = 0.0 → pure keyword search
    /// - alpha = 0.7 → hybrid (default, favors semantic)
    pub async fn search(&self, query: &str, top_k: usize, alpha: f64) -> Result<HybridResult> {
        // Get query embedding from Ollama
        let query_vec = match self.embed_bridge.embed(query).await {
            Ok(v) => v,
            Err(e) => {
                warn!("Query embedding failed: {}, using placeholder", e);
                self.placeholder_embedding(query)
            }
        };

        // Vector search
        let vector_results: Vec<SearchResult> =
            match self.vector_index.search(&query_vec, top_k * 2) {
                Ok(results) => results,
                Err(_) => Vec::new(),
            };

        // Keyword search
        let keyword_results = {
            let bm25 = self.keyword_index.lock().await;
            bm25.search(query, top_k * 2)
        };

        // Merge via Reciprocal Rank Fusion (RRF)
        let mut hits: std::collections::HashMap<String, SearchHit> =
            std::collections::HashMap::new();
        let k = 60;

        for (rank, result) in vector_results.iter().enumerate() {
            let rrf_score = alpha / (k + rank + 1) as f64;
            let meta = self.get_metadata(&result.label);
            let entry = hits
                .entry(result.label.clone())
                .or_insert_with(|| SearchHit {
                    id: result.label.clone(),
                    doc_id: meta.as_ref().map(|m| m.doc_id.clone()).unwrap_or_default(),
                    page_range: meta
                        .as_ref()
                        .map(|m| (m.start_page, m.end_page))
                        .unwrap_or((0, 0)),
                    vector_score: result.score,
                    keyword_score: 0.0,
                    hybrid_score: 0.0,
                    chunk_type: meta
                        .as_ref()
                        .map(|m| m.chunk_type)
                        .unwrap_or(ChunkType::Body),
                    text_preview: meta
                        .as_ref()
                        .map(|m| m.text_preview.clone())
                        .unwrap_or_default(),
                });
            entry.vector_score = result.score;
            entry.hybrid_score += rrf_score;
        }

        for (rank, (id, score)) in keyword_results.iter().enumerate() {
            let rrf_score = (1.0 - alpha) / (k + rank + 1) as f64;
            let meta = self.get_metadata(id);
            let entry = hits.entry(id.clone()).or_insert_with(|| SearchHit {
                id: id.clone(),
                doc_id: meta.as_ref().map(|m| m.doc_id.clone()).unwrap_or_default(),
                page_range: meta
                    .as_ref()
                    .map(|m| (m.start_page, m.end_page))
                    .unwrap_or((0, 0)),
                vector_score: 0.0,
                keyword_score: *score,
                hybrid_score: 0.0,
                chunk_type: meta
                    .as_ref()
                    .map(|m| m.chunk_type)
                    .unwrap_or(ChunkType::Body),
                text_preview: meta
                    .as_ref()
                    .map(|m| m.text_preview.clone())
                    .unwrap_or_default(),
            });
            entry.keyword_score = *score;
            entry.hybrid_score += rrf_score;
        }

        let mut results: Vec<SearchHit> = hits.into_values().collect();
        results.sort_by(|a, b| {
            b.hybrid_score
                .partial_cmp(&a.hybrid_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(top_k);

        Ok(HybridResult {
            hits: results,
            total_vector_results: vector_results.len(),
            total_keyword_results: keyword_results.len(),
            query: query.to_string(),
        })
    }

    /// Get metadata for a chunk from Sled.
    fn get_metadata(&self, chunk_id: &str) -> Option<PageMetadata> {
        self.meta_db
            .get(chunk_id.as_bytes())
            .ok()
            .flatten()
            .and_then(|ivec| serde_json::from_slice::<PageMetadata>(&ivec).ok())
    }

    /// Placeholder embedding — deterministic hash-based vector.
    /// Used when Ollama is unavailable.
    fn placeholder_embedding(&self, text: &str) -> Vec<f32> {
        let mut v = vec![0.0f32; self.dim];
        for (i, byte) in text.bytes().enumerate() {
            let idx = i % self.dim;
            v[idx] += byte as f32 * 0.01;
        }
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            v.iter_mut().for_each(|x| *x /= norm);
        }
        v
    }

    /// Number of chunks in the store.
    pub fn len(&self) -> usize {
        self.vector_index.len()
    }

    pub fn is_empty(&self) -> bool {
        self.vector_index.is_empty()
    }

    /// Check if Ollama embedding is available.
    pub async fn is_embed_available(&self) -> bool {
        self.embed_bridge.health_check().await.unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::TextPageProvider;
    use tempfile::TempDir;

    fn make_store(dir: &std::path::Path) -> PageAwareStore {
        let path = dir.join("rag-store");
        std::fs::create_dir_all(&path).unwrap();
        let embed_config = EmbedConfig {
            ollama_url: "http://127.0.0.1:19999".to_string(),
            dimension: 64,
            timeout_secs: 2,
            ..Default::default()
        };
        PageAwareStore::open(&path, 64, PageConfig::default(), embed_config).unwrap()
    }

    #[test]
    fn page_config_custom() {
        let config = PageConfig {
            window_size: 3,
            overlap: 1,
            min_chunk_chars: 100,
            max_chunk_chars: 16000,
            clean_headers_footers: true,
        };
        assert_eq!(config.window_size, 3);
        assert_eq!(config.overlap, 1);
    }

    #[tokio::test]
    async fn ingest_with_placeholder() {
        let dir = TempDir::new().unwrap();
        let store = make_store(dir.path());
        // Create multiple pages explicitly with form feed
        let text = "Rust programming language focuses on memory safety and performance without garbage collection. The borrow checker ensures reference safety at compile time.\n\x0cPython is a popular scripting language used for data science, web development, and automation. It has a rich ecosystem of libraries.";
        let provider = TextPageProvider::from_text("doc-1", "Test Doc", text);
        eprintln!(
            "DEBUG: page_count={}, window={}, overlap={}",
            provider.get_page_count(),
            2,
            1
        );
        let count = store.ingest(&provider).await.unwrap();
        eprintln!("DEBUG: ingested {} chunks", count);
        assert!(
            count >= 1,
            "should ingest at least one chunk, got {}",
            count
        );
    }

    #[tokio::test]
    async fn search_with_placeholder() {
        let dir = TempDir::new().unwrap();
        let store = make_store(dir.path());
        let long_text = (0..30).map(|i| format!("Paragraph {} discusses Rust programming and systems design with memory safety guarantees.", i)).collect::<Vec<_>>().join("\n");
        let provider = TextPageProvider::from_text("doc-1", "Rust Guide", &long_text);
        store.ingest(&provider).await.unwrap();

        let results = store.search("Rust programming", 5, 0.7).await.unwrap();
        assert!(
            !results.hits.is_empty(),
            "should find results for 'Rust programming'"
        );
    }

    #[tokio::test]
    async fn empty_store_search() {
        let dir = TempDir::new().unwrap();
        let store = make_store(dir.path());
        let results = store.search("anything", 5, 0.7).await.unwrap();
        assert!(results.hits.is_empty());
    }

    #[tokio::test]
    async fn embed_bridge_unreachable() {
        let config = EmbedConfig {
            ollama_url: "http://127.0.0.1:19999".to_string(),
            timeout_secs: 2,
            ..Default::default()
        };
        let bridge = EmbedBridge::new(config);
        let healthy = bridge.health_check().await.unwrap();
        assert!(!healthy);
    }

    #[tokio::test]
    async fn ingest_falls_back_to_placeholder() {
        let dir = TempDir::new().unwrap();
        let config = EmbedConfig {
            ollama_url: "http://127.0.0.1:19999".to_string(), // unreachable
            timeout_secs: 2,
            ..Default::default()
        };
        let store = PageAwareStore::open(dir.path(), 64, PageConfig::default(), config).unwrap();
        let provider = TextPageProvider::from_text(
            "doc-1",
            "Fallback Test",
            "Content about neural networks and machine learning algorithms.",
        );
        // Should succeed with placeholder vectors even though Ollama is unreachable
        let count = store.ingest(&provider).await.unwrap();
        assert!(count > 0, "should ingest with placeholder fallback");
    }
}
