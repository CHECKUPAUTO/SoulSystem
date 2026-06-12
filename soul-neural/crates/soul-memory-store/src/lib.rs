use anyhow::Result;
use parking_lot::RwLock;
use std::sync::Arc;

/// A simple HNSW-like vector store with concurrent reads.
/// Uses hierarchical navigable small world graphs for approximate nearest neighbor search.
/// For simplicity, the current implementation uses brute-force with HNSW-compatible API.
/// A real HNSW implementation would use the `instant-distance` crate.
pub struct HnswVectorStore {
    dim: usize,
    /// All inserted vectors
    vectors: Vec<Vec<f32>>,
    /// Metadata serialized as bytes
    metadata: Vec<Vec<u8>>,
    /// Layer assignment for HNSW (simplified: all points go to layer 0)
    layers: Vec<usize>,
    /// Connectivity: for each node, list of neighbor ids per layer
    graph: Vec<Vec<Vec<usize>>>,
    max_connections: usize,
}

impl HnswVectorStore {
    pub fn new(dim: usize, max_connections: usize) -> Self {
        Self {
            dim,
            vectors: Vec::new(),
            metadata: Vec::new(),
            layers: Vec::new(),
            graph: Vec::new(),
            max_connections,
        }
    }

    /// Insert a vector with associated metadata.
    pub fn insert(&mut self, vector: Vec<f32>, data: Vec<u8>) -> usize {
        let id = self.vectors.len();
        assert_eq!(vector.len(), self.dim, "Vector dimension mismatch");
        self.vectors.push(vector);
        self.metadata.push(data);
        self.layers.push(0);

        // Build connections: find nearest neighbors via brute force
        let neighbors = if id > 0 {
            let mut dists: Vec<(f32, usize)> = (0..id)
                .map(|i| (euclidean_dist(&self.vectors[id], &self.vectors[i]), i))
                .collect();
            dists.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
            dists.truncate(self.max_connections);
            dists.iter().map(|&(_, idx)| idx).collect()
        } else {
            Vec::new()
        };

        // Update graph: add connections to neighbors
        self.graph.push(vec![neighbors.clone()]);

        // Also add reverse connections (bidirectional)
        for &neighbor_id in &neighbors {
            for layer in 0..self.graph[neighbor_id].len() {
                if self.graph[neighbor_id][layer].len() < self.max_connections {
                    self.graph[neighbor_id][layer].push(id);
                }
            }
        }
        id
    }

    /// Search for the k nearest neighbors.
    pub fn search(&self, query: &[f32], k: usize) -> Vec<(f32, usize)> {
        if self.vectors.is_empty() {
            return Vec::new();
        }

        let mut dists: Vec<(f32, usize)> = self
            .vectors
            .iter()
            .enumerate()
            .map(|(i, v)| (euclidean_dist(query, v), i))
            .collect();
        dists.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        dists.truncate(k);
        dists
    }

    /// Get metadata for a given id.
    pub fn get_metadata(&self, id: usize) -> Option<&[u8]> {
        self.metadata.get(id).map(|v| v.as_slice())
    }

    /// Number of stored vectors.
    pub fn len(&self) -> usize {
        self.vectors.len()
    }

    pub fn is_empty(&self) -> bool {
        self.vectors.is_empty()
    }

    pub fn dim(&self) -> usize {
        self.dim
    }
}

/// Thread-safe wrapper with concurrent read access.
pub struct SharedVectorStore {
    inner: Arc<RwLock<HnswVectorStore>>,
}

impl SharedVectorStore {
    pub fn new(dim: usize, max_connections: usize) -> Self {
        Self {
            inner: Arc::new(RwLock::new(HnswVectorStore::new(dim, max_connections))),
        }
    }

    pub fn insert(&self, vector: Vec<f32>, data: Vec<u8>) -> usize {
        self.inner.write().insert(vector, data)
    }

    pub fn search(&self, query: &[f32], k: usize) -> Vec<(f32, usize)> {
        self.inner.read().search(query, k)
    }

    pub fn get_metadata(&self, id: usize) -> Option<Vec<u8>> {
        self.inner.read().get_metadata(id).map(|v| v.to_vec())
    }

    pub fn len(&self) -> usize {
        self.inner.read().len()
    }

    pub fn dim(&self) -> usize {
        self.inner.read().dim()
    }
}

fn euclidean_dist(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b)
        .map(|(x, y)| (x - y).powi(2))
        .sum::<f32>()
        .sqrt()
}

lazy_static::lazy_static! {
    static ref MEMORY_SIZE: prometheus::IntGauge = prometheus::register_int_gauge!(
        "memory_store_size", "Number of episodes in memory store"
    ).unwrap();
    static ref MEMORY_SEARCH_DURATION: prometheus::Histogram = prometheus::register_histogram!(
        "memory_search_duration_seconds", "Duration of search operations",
        vec![1e-5, 5e-5, 1e-4, 5e-4, 1e-3, 5e-3]
    ).unwrap();
    static ref MEMORY_INSERT_DURATION: prometheus::Histogram = prometheus::register_histogram!(
        "memory_insert_duration_seconds", "Duration of insert operations",
        vec![1e-5, 5e-5, 1e-4, 5e-4, 1e-3]
    ).unwrap();
}

impl HnswVectorStore {
    pub fn save(&self, path: &str) -> Result<()> {
        let data = bincode::serialize(&self.vectors)?;
        let meta = bincode::serialize(&self.metadata)?;
        let graph = bincode::serialize(&self.graph)?;
        let combined = (data, meta, graph, self.dim, self.max_connections);
        let bin = bincode::serialize(&combined)?;
        std::fs::write(path, bin)?;
        log::info!("Saved {} vectors to {}", self.vectors.len(), path);
        Ok(())
    }

    pub fn load(path: &str) -> Result<Self> {
        let bin = std::fs::read(path)?;
        let (data_bytes, meta_bytes, graph_bytes, dim, max_conn): (
            Vec<u8>,
            Vec<u8>,
            Vec<u8>,
            usize,
            usize,
        ) = bincode::deserialize(&bin)?;
        let vectors: Vec<Vec<f32>> = bincode::deserialize(&data_bytes)?;
        let metadata: Vec<Vec<u8>> = bincode::deserialize(&meta_bytes)?;
        let graph: Vec<Vec<Vec<usize>>> = bincode::deserialize(&graph_bytes)?;
        let layers = vec![0; vectors.len()];
        Ok(Self {
            vectors,
            metadata,
            layers,
            graph,
            max_connections: max_conn,
            dim,
        })
    }
}

impl SharedVectorStore {
    pub fn save(&self, path: &str) -> Result<()> {
        self.inner.read().save(path)
    }

    pub fn load(path: &str, _dim: usize, _max_connections: usize) -> Result<Self> {
        let inner = HnswVectorStore::load(path)?;
        Ok(Self {
            inner: Arc::new(RwLock::new(inner)),
        })
    }
}

/// Insert with metric tracking
impl SharedVectorStore {
    pub fn insert_tracked(&self, vector: Vec<f32>, data: Vec<u8>) -> usize {
        let _t = MEMORY_INSERT_DURATION.start_timer();
        let id = self.insert(vector, data);
        MEMORY_SIZE.inc();
        id
    }

    pub fn search_tracked(&self, query: &[f32], k: usize) -> Vec<(f32, usize)> {
        let _t = MEMORY_SEARCH_DURATION.start_timer();
        self.search(query, k)
    }
}

#[cfg(test)]
mod store_tests {
    use super::*;

    #[test]
    fn test_persistence() {
        let mut store = HnswVectorStore::new(3, 10);
        store.insert(vec![1.0, 0.0, 0.0], b"ep1".to_vec());
        store.insert(vec![0.0, 1.0, 0.0], b"ep2".to_vec());

        let tmp = std::env::temp_dir().join("test_hnsw.bin");
        let path = tmp.to_str().unwrap();
        store.save(path).unwrap();

        let loaded = HnswVectorStore::load(path).unwrap();
        assert_eq!(loaded.len(), 2);
        let results = loaded.search(&[1.0, 0.1, 0.0], 1);
        assert_eq!(results.len(), 1);
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn test_dedup_shared() {
        let shared = SharedVectorStore::new(3, 10);
        let id1 = shared.insert(vec![1.0, 0.0, 0.0], b"same".to_vec());
        let id2 = shared.insert(vec![1.0, 0.0, 0.0], b"same".to_vec());
        assert_ne!(
            id1, id2,
            "Should insert both, dedup is caller responsibility"
        );
    }
}
