//! CCOS → OctaSoma cascade — causal + semantic context for SoulSystem.
//!
//!   1. **Causal** (CCOS) — narrow a query to its causal region.
//!   2. **Semantic** (OctaSoma) — cosine rerank within the region.
//!
//! Both compose in `CascadeMemory::recall()`.

use crate::ccos_bridge::CausalMemoryBackend;
use octasoma::{Embedder, FractalMemory3D, HashEmbedder};

/// Errors from the cascade layer.
#[derive(Debug)]
pub enum CascadeError {
    Causal(String),
    Semantic(String),
}

impl std::fmt::Display for CascadeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CascadeError::Causal(msg) => write!(f, "Causal error: {msg}"),
            CascadeError::Semantic(msg) => write!(f, "Semantic error: {msg}"),
        }
    }
}

impl std::error::Error for CascadeError {}

/// CCOS → OctaSoma cascade memory.
///
/// 1. CCOS identifies the causal region around a failing file.
/// 2. OctaSoma reranks that region semantically for the query.
pub struct CascadeMemory {
    causal: CausalMemoryBackend,
    semantic: FractalMemory3D,
    embedder: HashEmbedder,
}

impl CascadeMemory {
    /// Create a new cascade with hash-based embeddings (offline).
    pub fn new(dim: usize) -> Self {
        Self {
            causal: CausalMemoryBackend::new(),
            semantic: FractalMemory3D::new(dim, 42),
            embedder: HashEmbedder::new(dim),
        }
    }

    /// Create with a PCA-calibrated OctaSoma engine.
    pub fn with_pca(dim: usize, calibration: &[f32], num_samples: usize) -> Self {
        Self {
            causal: CausalMemoryBackend::new(),
            semantic: FractalMemory3D::new_with_pca(dim, calibration, num_samples),
            embedder: HashEmbedder::new(dim),
        }
    }

    /// Ingest a source file into both memories.
    pub fn ingest_source(&mut self, uri: &str, source: &str) {
        self.causal.ingest_source(uri, source);
        if let Ok(embedding) = self.embedder.embed(source) {
            self.semantic.insert(&embedding, Some(source.as_bytes()));
        }
    }

    /// Recall: CCOS causal scope → OctaSoma semantic rerank.
    pub fn recall(
        &self,
        failing_file: &str,
        query: &str,
        budget: usize,
    ) -> Result<String, CascadeError> {
        // Step 1: CCOS causal context window
        let causal_ctx = self.causal.recall_text(failing_file, budget);

        // Step 2: OctaSoma semantic rerank
        let sem_hits: Vec<&[u8]> = self
            .embedder
            .embed(query)
            .map(|qemb| self.semantic.query_k(&qemb, 5))
            .unwrap_or_default();

        // Build result
        let mut result = String::new();
        result.push_str(&format!(
            "// CCOS → OctaSoma cascade\n// failing: {failing_file}\n// query: {query}\n// budget: {budget} tok\n\n"
        ));
        result.push_str("// ── causal scope (CCOS) ──\n");
        result.push_str(&causal_ctx.text);

        if !sem_hits.is_empty() {
            result.push_str("\n// ── semantic rerank (OctaSoma) ──\n");
            for (i, hit) in sem_hits.iter().enumerate() {
                if let Ok(text) = std::str::from_utf8(hit) {
                    if text.len() < 200 {
                        result.push_str(&format!("//   [{i}] {text}\n"));
                    }
                }
            }
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cascade_basic() {
        let mut cascade = CascadeMemory::new(64);
        cascade.ingest_source("src/lib.rs", "pub fn run() -> u8 { config::get() }");
        cascade.ingest_source("src/config.rs", "pub fn get() -> u8 { 42 }");

        let result = cascade
            .recall("src/lib.rs", "run the function", 1024)
            .unwrap();
        assert!(
            result.contains("config"),
            "causal scope should include config.rs"
        );
        assert!(
            result.contains("causal scope"),
            "should mark causal section"
        );
    }
}
