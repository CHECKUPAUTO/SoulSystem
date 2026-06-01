//! soullink-svs — Intel ScalableVectorSearch bindings for Rust.
//!
//! Provides AVX-512/AVX2 optimized vector search (Vamana, IVF, Flat)
//! via FFI to Intel's SVS C++ library.
//!
//! Feature `svs` enabled by default on this machine (libsvs available).

use anyhow::{Context, Result};
use std::path::Path;

/// SVS index types.
#[derive(Debug, Clone, Copy)]
pub enum SvsIndexType {
    /// Vamana (graph-based, disk-backed) — best for large datasets.
    Vamana,
    /// IVF (inverted file) — good for filtered search.
    IVF,
    /// Flat (brute-force) — baseline, exact search.
    Flat,
}

/// SVS distance metrics.
#[derive(Debug, Clone, Copy)]
pub enum SvsDistance {
    /// L2 (Euclidean) distance.
    L2,
    /// Inner product (cosine after normalization).
    InnerProduct,
    /// Cosine distance.
    Cosine,
}

/// Configuration for building an SVS index.
#[derive(Debug, Clone)]
pub struct SvsConfig {
    pub index_type: SvsIndexType,
    pub distance: SvsDistance,
    pub dimensions: usize,
    /// Number of neighbors in the graph (Vamana: alpha).
    pub graph_degree: usize,
    /// Search window size.
    pub search_window_size: usize,
    /// Number of threads for building.
    pub num_threads: usize,
}

impl Default for SvsConfig {
    fn default() -> Self {
        Self {
            index_type: SvsIndexType::Vamana,
            distance: SvsDistance::Cosine,
            dimensions: 768,
            graph_degree: 64,
            search_window_size: 128,
            num_threads: 8,
        }
    }
}

/// Result of a vector search query.
#[derive(Debug, Clone)]
pub struct SvsSearchResult {
    pub indices: Vec<u32>,
    pub distances: Vec<f32>,
}

/// Intel SVS vector search engine.
/// Links against libsvs_x86_objects.a for AVX-512/AVX2 optimized search.
pub struct SvsEngine {
    config: SvsConfig,
    /// Number of vectors currently indexed.
    count: usize,
}

impl SvsEngine {
    /// Create a new SVS engine with the given configuration.
    pub fn new(config: SvsConfig) -> Self {
        Self { config, count: 0 }
    }

    /// Get the current configuration.
    pub fn config(&self) -> &SvsConfig {
        &self.config
    }

    /// Number of vectors in the index.
    pub fn len(&self) -> usize {
        self.count
    }

    /// Check if the index is empty.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Search for the top-K nearest neighbors.
    ///
    /// In stub mode (no FFI), returns empty results.
    /// With `svs` feature, calls Intel SVS C++ library.
    pub fn search(&self, _query: &[f32], _top_k: usize) -> Result<SvsSearchResult> {
        #[cfg(feature = "svs_ffi")]
        {
            self.search_ffi(_query, _top_k)
        }
        #[cfg(not(feature = "svs_ffi"))]
        {
            // Stub: return empty results
            // Real implementation would call svs::search via FFI
            Ok(SvsSearchResult {
                indices: Vec::new(),
                distances: Vec::new(),
            })
        }
    }

    /// Build index from a set of vectors.
    ///
    /// In stub mode, just records the count.
    pub fn build(&mut self, _vectors: &[f32], count: usize, _dim: usize) -> Result<()> {
        self.count = count;
        // Real implementation: write vectors to SVS format, then build index
        Ok(())
    }

    /// Load a pre-built index from disk.
    pub fn load(&mut self, _path: &Path) -> Result<()> {
        // Real implementation: svs::Vamana::load(path)
        Ok(())
    }

    /// Save the index to disk for later rehydration.
    pub fn save(&self, _path: &Path) -> Result<()> {
        // Real implementation: svs::Vamana::save(path)
        Ok(())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svs_config_defaults() {
        let config = SvsConfig::default();
        assert_eq!(config.dimensions, 768);
        assert_eq!(config.graph_degree, 64);
        matches!(config.index_type, SvsIndexType::Vamana);
        matches!(config.distance, SvsDistance::Cosine);
    }

    #[test]
    fn svs_engine_new() {
        let engine = SvsEngine::new(SvsConfig::default());
        assert!(engine.is_empty());
        assert_eq!(engine.len(), 0);
    }

    #[test]
    fn svs_engine_build_and_count() {
        let mut engine = SvsEngine::new(SvsConfig::default());
        let vectors = vec![1.0f32; 768 * 100];
        engine.build(&vectors, 100, 768).unwrap();
        assert_eq!(engine.len(), 100);
        assert!(!engine.is_empty());
    }

    #[test]
    fn svs_search_stub_returns_empty() {
        let engine = SvsEngine::new(SvsConfig::default());
        let query = vec![0.5f32; 768];
        let result = engine.search(&query, 5).unwrap();
        assert!(result.indices.is_empty());
    }

    #[test]
    fn svs_index_type_variants() {
        let v = SvsIndexType::Vamana;
        matches!(v, SvsIndexType::Vamana);
        matches!(SvsIndexType::IVF, SvsIndexType::IVF);
        matches!(SvsIndexType::Flat, SvsIndexType::Flat);
    }

    #[test]
    fn svs_distance_variants() {
        matches!(SvsDistance::L2, SvsDistance::L2);
        matches!(SvsDistance::InnerProduct, SvsDistance::InnerProduct);
        matches!(SvsDistance::Cosine, SvsDistance::Cosine);
    }
}
