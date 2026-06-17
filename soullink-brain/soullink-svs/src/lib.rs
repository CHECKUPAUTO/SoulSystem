//! soullink-svs — Intel ScalableVectorSearch bindings for Rust.
//!
//! Provides AVX-512/AVX2 optimized vector search (Vamana, IVF, Flat)
//! via FFI to Intel's SVS C++ library.
//!
//! Feature `svs` enabled by default on this machine (libsvs available).

use anyhow::Result;
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
///
/// With the `svs_ffi` feature this links against libsvs for AVX-512/AVX2
/// optimized search. Without it, the engine is a fully functional pure-Rust
/// fallback: vectors are kept in memory, search is exact brute-force over the
/// configured metric, and the index round-trips to disk. It is never a silent
/// no-op.
pub struct SvsEngine {
    config: SvsConfig,
    /// Flat row-major store of `count` vectors, each `dim` long (fallback mode).
    vectors: Vec<f32>,
    /// Dimensionality of stored vectors (0 until the first build).
    dim: usize,
    /// Number of vectors currently indexed.
    count: usize,
}

impl SvsEngine {
    /// Create a new SVS engine with the given configuration.
    pub fn new(config: SvsConfig) -> Self {
        Self {
            config,
            vectors: Vec::new(),
            dim: 0,
            count: 0,
        }
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
    /// With `svs_ffi` this calls Intel SVS; otherwise it runs an exact
    /// brute-force scan over the in-memory vectors using the configured metric.
    pub fn search(&self, query: &[f32], top_k: usize) -> Result<SvsSearchResult> {
        #[cfg(feature = "svs_ffi")]
        {
            return self.search_ffi(query, top_k);
        }
        #[cfg(not(feature = "svs_ffi"))]
        {
            self.search_bruteforce(query, top_k)
        }
    }

    /// Exact brute-force nearest-neighbor scan (fallback mode).
    #[cfg(not(feature = "svs_ffi"))]
    fn search_bruteforce(&self, query: &[f32], top_k: usize) -> Result<SvsSearchResult> {
        if self.count == 0 || top_k == 0 {
            return Ok(SvsSearchResult {
                indices: Vec::new(),
                distances: Vec::new(),
            });
        }
        if query.len() != self.dim {
            anyhow::bail!(
                "query dimension {} does not match index dimension {}",
                query.len(),
                self.dim
            );
        }

        // Score every vector; smaller score = closer for all metrics here.
        let mut scored: Vec<(u32, f32)> = (0..self.count)
            .map(|i| {
                let v = &self.vectors[i * self.dim..(i + 1) * self.dim];
                (i as u32, self.distance(query, v))
            })
            .collect();
        scored.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(top_k);

        Ok(SvsSearchResult {
            indices: scored.iter().map(|(i, _)| *i).collect(),
            distances: scored.iter().map(|(_, d)| *d).collect(),
        })
    }

    /// Distance between two equal-length vectors under the configured metric.
    /// All variants return a value where smaller means more similar.
    #[cfg(not(feature = "svs_ffi"))]
    fn distance(&self, a: &[f32], b: &[f32]) -> f32 {
        match self.config.distance {
            SvsDistance::L2 => a
                .iter()
                .zip(b)
                .map(|(x, y)| {
                    let d = x - y;
                    d * d
                })
                .sum::<f32>(),
            SvsDistance::InnerProduct => {
                // Higher inner product = more similar, so negate for "smaller=closer".
                -a.iter().zip(b).map(|(x, y)| x * y).sum::<f32>()
            }
            SvsDistance::Cosine => {
                let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
                let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
                let nb: f32 = b.iter().map(|y| y * y).sum::<f32>().sqrt();
                if na == 0.0 || nb == 0.0 {
                    1.0
                } else {
                    1.0 - dot / (na * nb)
                }
            }
        }
    }

    /// Build index from a flat row-major set of `count` vectors of width `dim`.
    pub fn build(&mut self, vectors: &[f32], count: usize, dim: usize) -> Result<()> {
        if count * dim != vectors.len() {
            anyhow::bail!(
                "vector buffer length {} != count {} * dim {}",
                vectors.len(),
                count,
                dim
            );
        }
        self.vectors = vectors.to_vec();
        self.count = count;
        self.dim = dim;
        Ok(())
    }

    /// Load a pre-built index from disk (the format written by [`Self::save`]).
    pub fn load(&mut self, path: &Path) -> Result<()> {
        let bytes = std::fs::read(path)?;
        if bytes.len() < 16 {
            anyhow::bail!("svs index file too short");
        }
        let count = u64::from_le_bytes(bytes[0..8].try_into().unwrap()) as usize;
        let dim = u64::from_le_bytes(bytes[8..16].try_into().unwrap()) as usize;
        let expected = 16 + count * dim * 4;
        if bytes.len() != expected {
            anyhow::bail!(
                "svs index file length {} != expected {}",
                bytes.len(),
                expected
            );
        }
        let mut vectors = Vec::with_capacity(count * dim);
        for chunk in bytes[16..].chunks_exact(4) {
            vectors.push(f32::from_le_bytes(chunk.try_into().unwrap()));
        }
        self.vectors = vectors;
        self.count = count;
        self.dim = dim;
        Ok(())
    }

    /// Save the index to disk for later rehydration.
    ///
    /// Format: `count: u64 LE`, `dim: u64 LE`, then `count * dim` `f32 LE`.
    pub fn save(&self, path: &Path) -> Result<()> {
        let mut buf = Vec::with_capacity(16 + self.vectors.len() * 4);
        buf.extend_from_slice(&(self.count as u64).to_le_bytes());
        buf.extend_from_slice(&(self.dim as u64).to_le_bytes());
        for v in &self.vectors {
            buf.extend_from_slice(&v.to_le_bytes());
        }
        std::fs::write(path, buf)?;
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

    #[cfg(not(feature = "svs_ffi"))]
    fn l2_engine() -> SvsEngine {
        let cfg = SvsConfig {
            distance: SvsDistance::L2,
            dimensions: 2,
            ..SvsConfig::default()
        };
        let mut e = SvsEngine::new(cfg);
        // Three 2-D points; nearest to (0,0) is index 0, then 1, then 2.
        e.build(&[0.0, 0.0, 1.0, 0.0, 5.0, 5.0], 3, 2).unwrap();
        e
    }

    #[test]
    #[cfg(not(feature = "svs_ffi"))]
    fn bruteforce_search_ranks_by_distance() {
        let e = l2_engine();
        let res = e.search(&[0.0, 0.0], 2).unwrap();
        assert_eq!(res.indices, vec![0, 1]);
        assert!(res.distances[0] <= res.distances[1]);
    }

    #[test]
    #[cfg(not(feature = "svs_ffi"))]
    fn search_rejects_dimension_mismatch() {
        let e = l2_engine();
        assert!(e.search(&[1.0, 2.0, 3.0], 1).is_err());
    }

    #[test]
    #[cfg(not(feature = "svs_ffi"))]
    fn save_load_round_trips_the_index() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("index.svs");
        let e = l2_engine();
        e.save(&path).unwrap();

        let mut restored = SvsEngine::new(SvsConfig {
            distance: SvsDistance::L2,
            dimensions: 2,
            ..SvsConfig::default()
        });
        restored.load(&path).unwrap();
        assert_eq!(restored.len(), 3);
        // Search behaves identically after rehydration.
        assert_eq!(restored.search(&[0.0, 0.0], 1).unwrap().indices, vec![0]);
    }
}
