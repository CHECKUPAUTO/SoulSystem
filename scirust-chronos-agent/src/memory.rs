// ==========================================================================
// memory.rs — AtemporalMemory (Pillar 2)
//
// Dual-store architecture: episodic (short-term trajectory) and semantic
// (long-term patterns).  Both stores support cosine-similarity search with
// Shannon-entropy-based synchrony index α_sync.
//
// Vector search is parallelised via rayon as a safe fallback; the
// VectorIndex trait is ready for faiss integration.
// ==========================================================================

use rayon::prelude::*;
use std::f64::consts::LN_2;

// --------------------------------------------------------------------------
// VectorIndex trait — abstraction over similarity search backends
// --------------------------------------------------------------------------

pub trait VectorIndex: Send + Sync {
    /// Dimension of stored vectors.
    fn dim(&self) -> usize;

    /// Number of vectors currently stored.
    fn len(&self) -> usize;

    fn is_empty(&self) -> bool { self.len() == 0 }

    /// Insert a new vector into the index.
    fn insert(&mut self, key: Vec<f64>);

    /// Search for the top-`k` nearest neighbours of `query`, returning
    /// (index, cosine_similarity) pairs sorted descending.
    fn search(&self, query: &[f64], k: usize) -> Vec<(usize, f64)>;

    /// Return the raw vector at `index`.
    fn get(&self, index: usize) -> Option<&[f64]>;
}

// --------------------------------------------------------------------------
// BruteForceIndex — parallel linear scan via rayon
// --------------------------------------------------------------------------

pub struct BruteForceIndex {
    dim: usize,
    pub vectors: Vec<Vec<f64>>,
}

impl BruteForceIndex {
    pub fn new(dim: usize) -> Self {
        Self { dim, vectors: Vec::with_capacity(1024) }
    }
}

impl VectorIndex for BruteForceIndex {
    fn dim(&self) -> usize { self.dim }

    fn len(&self) -> usize { self.vectors.len() }

    fn insert(&mut self, key: Vec<f64>) {
        debug_assert_eq!(key.len(), self.dim);
        self.vectors.push(key);
    }

    fn search(&self, query: &[f64], k: usize) -> Vec<(usize, f64)> {
        if self.vectors.is_empty() {
            return Vec::new();
        }
        let query_norm = dot(query, query).sqrt().max(f64::EPSILON);

        // Parallel cosine similarity via rayon
        let mut scores: Vec<(usize, f64)> = self.vectors
            .par_iter()
            .enumerate()
            .map(|(i, vec)| {
                let dot_val = dot(query, vec);
                let vec_norm = dot(vec, vec).sqrt().max(f64::EPSILON);
                (i, dot_val / (query_norm * vec_norm))
            })
            .collect();

        // Sort descending by similarity
        scores.par_sort_unstable_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        scores.truncate(k);
        scores
    }

    fn get(&self, index: usize) -> Option<&[f64]> {
        self.vectors.get(index).map(|v| v.as_slice())
    }
}

// --------------------------------------------------------------------------
// Helpers
// --------------------------------------------------------------------------

#[inline]
fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

/// Shannon entropy of a probability distribution.
pub fn shannon_entropy(probs: &[f64]) -> f64 {
    let mut h = 0.0;
    for &p in probs {
        if p > 0.0 {
            h -= p * p.ln() / LN_2;
        }
    }
    h
}

/// Softmax transform with temperature.
pub fn softmax(logits: &[f64], temperature: f64) -> Vec<f64> {
    let max_val = logits.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let exps: Vec<f64> = logits
        .iter()
        .map(|&x| ((x - max_val) / temperature.max(f64::EPSILON)).exp())
        .collect();
    let sum: f64 = exps.iter().sum();
    exps.into_iter().map(|e| e / sum).collect()
}

// --------------------------------------------------------------------------
// EpisodicStore — short-horizon trajectory buffer
// --------------------------------------------------------------------------

pub struct EpisodicStore {
    pub index: BruteForceIndex,
    pub capacity: usize,
}

impl EpisodicStore {
    pub fn new(dim: usize, capacity: usize) -> Self {
        Self { index: BruteForceIndex::new(dim), capacity }
    }

    pub fn push(&mut self, vec: Vec<f64>) {
        if self.index.len() >= self.capacity {
            self.index.vectors.remove(0);
        }
        self.index.insert(vec);
    }

    pub fn synchrony(&self, query: &[f64]) -> f64 {
        let k = self.index.len().min(8).max(1);
        let results = self.index.search(query, k);
        if results.is_empty() {
            return 0.0;
        }
        // V2: absolute similarity threshold — mean cosine similarity of top-k.
        // This decouples "memory diversity" from "memory stability".
        // Entropy-based α was too sensitive to input diversity (5 clusters → H high → α low).
        let mean_sim: f64 = results.iter().map(|&(_, s)| s).sum::<f64>() / k as f64;
        // Apply soft threshold τ: similarity > 0.75 → stable territory
        let tau: f64 = 0.75;
        // Sigmoid-like mapping: α = 1 / (1 + exp(-10 * (mean_sim - tau)))
        // Sharp transition around τ, bounded [0, 1]
        1.0 / (1.0 + (-10.0 * (mean_sim - tau)).exp())
    }
}

// --------------------------------------------------------------------------
// HNSWIndex — Hierarchical Navigable Small World index
//
// Multi-layer graph navigation for O(log n) approximate nearest neighbour
// search.  Reference: Malkov & Yashunin (2018), "Efficient and robust
// approximate nearest neighbor search using Hierarchical Navigable Small
// World graphs".
//
// Key parameters:
//   M         — number of bi-directional connections per layer (default: 16)
//   M_max     — max connections per layer (2*M)
//   ef_search — search width (default: 64)
//   ef_insert — insertion width (default: 128)
//   ml        — normalisation factor for layer assignment (~1/ln(M))
// --------------------------------------------------------------------------

use std::collections::HashSet;

/// A single node in the HNSW graph.
struct HNSWNode {
    vector: Vec<f64>,
    /// Connections per layer: layers[layer] = vec of neighbour indices
    layers: Vec<Vec<usize>>,
    /// The highest layer this node appears on.
    max_layer: usize,
}

pub struct HNSWIndex {
    dim: usize,
    nodes: Vec<HNSWNode>,
    /// Entry point (node with highest layer).
    entry_point: usize,
    /// Maximum layer across all nodes.
    max_layer: usize,
    // Parameters
    m: usize,
    m_max: usize,
    ef_search: usize,
    ef_insert: usize,
    ml: f64,
}

impl HNSWIndex {
    pub fn new(dim: usize) -> Self {
        Self {
            dim,
            nodes: Vec::new(),
            entry_point: 0,
            max_layer: 0,
            m: 16,
            m_max: 32,
            ef_search: 64,
            ef_insert: 128,
            ml: 1.0 / (16.0f64).ln(),
        }
    }

    pub fn with_params(dim: usize, m: usize, ef_search: usize, ef_insert: usize) -> Self {
        Self {
            dim,
            nodes: Vec::new(),
            entry_point: 0,
            max_layer: 0,
            m,
            m_max: m * 2,
            ef_search,
            ef_insert,
            ml: 1.0 / (m as f64).ln(),
        }
    }

    fn assign_layer(&self) -> usize {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let r: f64 = rng.gen();
        (-r.ln() * self.ml).floor() as usize
    }

    /// Search on a single layer, starting from `entry`.
    /// Returns the closest node found.
    fn search_layer(&self, query: &[f64], entry: usize, layer: usize) -> usize {
        let mut best = entry;
        let mut best_dist = 1.0 - cosine_sim(query, &self.nodes[entry].vector);

        loop {
            let mut improved = false;
            for &neighbour in &self.nodes[best].layers[layer] {
                let d = 1.0 - cosine_sim(query, &self.nodes[neighbour].vector);
                if d < best_dist {
                    best_dist = d;
                    best = neighbour;
                    improved = true;
                }
            }
            if !improved { break; }
        }
        best
    }

    /// Multi-layer search returning top-k nearest neighbours.
    fn search_impl(&self, query: &[f64], k: usize, ef: usize) -> Vec<(usize, f64)> {
        if self.nodes.is_empty() { return Vec::new(); }

        let ef_use = ef.max(k);

        // Start from entry point, traverse top layers greedily
        let mut curr = self.entry_point;
        for layer in (1..=self.max_layer).rev() {
            curr = self.search_layer(query, curr, layer);
        }

        // Layer 0: collect neighbours with ef width
        let mut candidates: Vec<(usize, f64)> = Vec::new();
        let mut visited: HashSet<usize> = HashSet::new();
        visited.insert(curr);
        let dist = 1.0 - cosine_sim(query, &self.nodes[curr].vector);
        candidates.push((curr, dist));

        let mut idx = 0;
        while idx < candidates.len() {
            let (candidate, _) = candidates[idx];
            idx += 1;

            for &neighbour in &self.nodes[candidate].layers[0] {
                if visited.insert(neighbour) {
                    let d = 1.0 - cosine_sim(query, &self.nodes[neighbour].vector);
                    candidates.push((neighbour, d));
                }
            }

            // If we have enough candidates and the farthest is close enough, stop
            if candidates.len() >= ef_use {
                candidates.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
                candidates.truncate(ef_use);
                idx = idx.min(candidates.len());
            }
        }

        candidates.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        candidates.truncate(k);

        // Convert distance back to cosine similarity
        candidates.into_iter()
            .map(|(idx, dist)| (idx, (1.0 - dist).clamp(-1.0, 1.0)))
            .collect()
    }

    /// Connect a new node to its nearest neighbours on each layer up to `max_l`.
    fn connect_new_node(&mut self, new_idx: usize, query: &[f64], max_l: usize) {
        let m_layer = if max_l == 0 { self.m_max } else { self.m };

        for layer in 0..=max_l {
            let neighbours = self.search_impl(query, m_layer, self.ef_insert);
            // Take top m_layer connections
            let top_n: Vec<usize> = neighbours.into_iter()
                .take(m_layer)
                .map(|(idx, _)| idx)
                .collect();

            // Bi-directional connection
            for &n_idx in &top_n {
                if n_idx != new_idx {
                    self.nodes[new_idx].layers[layer].push(n_idx);
                    // Add reverse connection if within capacity
                    if self.nodes[n_idx].layers[layer].len() < self.m_max {
                        self.nodes[n_idx].layers[layer].push(new_idx);
                    } else if layer == 0 {
                        // Shrink connections on layer 0 if over capacity
                        // (Simplified: just keep existing)
                    }
                }
            }
        }
    }
}

impl VectorIndex for HNSWIndex {
    fn dim(&self) -> usize { self.dim }
    fn len(&self) -> usize { self.nodes.len() }

    fn insert(&mut self, key: Vec<f64>) {
        let new_layer = self.assign_layer().min(self.max_layer + 1);
        let new_idx = self.nodes.len();

        let mut node = HNSWNode {
            vector: key.clone(),
            layers: vec![Vec::new(); new_layer + 1],
            max_layer: new_layer,
        };
        self.nodes.push(node);

        if new_idx == 0 {
            // First node
            self.entry_point = 0;
            self.max_layer = new_layer;
            return;
        }

        // Update entry point if new node has higher layer
        if new_layer > self.max_layer {
            self.max_layer = new_layer;
            self.entry_point = new_idx;
        }

        self.connect_new_node(new_idx, &key, new_layer);
    }

    fn search(&self, query: &[f64], k: usize) -> Vec<(usize, f64)> {
        self.search_impl(query, k, self.ef_search)
    }

    fn get(&self, index: usize) -> Option<&[f64]> {
        self.nodes.get(index).map(|n| n.vector.as_slice())
    }
}

fn cosine_sim(a: &[f64], b: &[f64]) -> f64 {
    let dot: f64 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let na: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt().max(f64::EPSILON);
    let nb: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt().max(f64::EPSILON);
    dot / (na * nb)
}

// --------------------------------------------------------------------------
// SemanticStore — now uses HNSWIndex
// --------------------------------------------------------------------------

pub struct SemanticStore {
    pub index: HNSWIndex,
    /// Number of times each entry has been reinforced.
    pub strengths: Vec<f64>,
}

impl SemanticStore {
    pub fn new(dim: usize) -> Self {
        Self { index: HNSWIndex::new(dim), strengths: Vec::new() }
    }

    /// Insert or reinforce.  If cosine similarity with existing exceeds
    /// threshold, reinforce instead of inserting.
    ///
    /// Strength is capped via amortised growth (diminishing returns)
    /// to prevent a single prototype from becoming a "cognitive black hole"
    /// that dominates all recalls.
    pub fn learn(&mut self, vec: Vec<f64>, threshold: f64) {
        if let Some((idx, sim)) = self.index.search(&vec, 1).into_iter().next() {
            if sim > threshold {
                // Amortised strength: fast early, saturating asymptotically
                let max_cap = 100.0f64;
                let delta = 1.0;
                let old = self.strengths[idx];
                self.strengths[idx] = old + delta / (1.0 + old / max_cap);
                // Exponential moving average of the stored prototype
                // HNSW: cannot mutate vectors in-place through trait, so we
                // re-insert the averaged version (the old index stays stale
                // until the next search picks up the new insertion).
                if let Some(stored) = self.index.get(idx) {
                    let lr = 0.1;
                    let mut averaged: Vec<f64> = stored.to_vec();
                    for (s, &v) in averaged.iter_mut().zip(&vec) {
                        *s += lr * (v - *s);
                    }
                    // Insert averaged prototype as a new entry
                    self.index.insert(averaged);
                    self.strengths.push(1.0);
                }
                return;
            }
        }
        self.index.insert(vec);
        self.strengths.push(1.0);
    }

    /// Retrieve top-k prototypes weighted by strength × similarity.
    pub fn recall(&self, query: &[f64], k: usize) -> Vec<(Vec<f64>, f64)> {
        let mut results = self.index.search(query, k);
        results.par_sort_unstable_by(|a, b| {
            let sa = a.1 * self.strengths.get(a.0).copied().unwrap_or(1.0);
            let sb = b.1 * self.strengths.get(b.0).copied().unwrap_or(1.0);
            sb.partial_cmp(&sa).unwrap()
        });
        results
            .into_iter()
            .filter_map(|(idx, sim)| {
                self.index.get(idx).map(|v| (v.to_vec(), sim))
            })
            .collect()
    }
}

// --------------------------------------------------------------------------
// AtemporalMemory — unified dual-store manager
// --------------------------------------------------------------------------

/// Hysteresis constant: alpha_sync decays at most `ALPHA_DECAY` per step.
/// Prevents catastrophic synchrony collapse under noise.
const ALPHA_DECAY: f64 = 0.15;

/// Alpha_sync floor — never goes below this, preserving minimal memory traction.
const ALPHA_FLOOR: f64 = 0.01;

pub struct AtemporalMemory {
    pub episodic: EpisodicStore,
    pub semantic: SemanticStore,
    pub alpha_sync: f64,     // synchrony index (updated each step, smoothed)
    pub alpha_raw: f64,      // raw instantaneous synchrony before smoothing
}

impl AtemporalMemory {
    pub fn new(dim: usize, episodic_capacity: usize) -> Self {
        Self {
            episodic: EpisodicStore::new(dim, episodic_capacity),
            semantic: SemanticStore::new(dim),
            alpha_sync: 1.0,
            alpha_raw: 1.0,
        }
    }

    /// Push the latest latent state into both stores and recompute α_sync.
    ///
    /// The published alpha_sync is smoothed with an inertial decay:
    ///   α(t) = clamp( α_raw(t), α(t-1) - ALPHA_DECAY, α(t-1) + ALPHA_DECAY )
    ///   α(t) = max(α(t), ALPHA_FLOOR)
    ///
    /// This prevents catastrophic synchrony collapse in a single step
    /// (e.g. white-noise burst) while allowing genuine desync to register.
    pub fn observe(&mut self, latent: &[f64]) {
        self.episodic.push(latent.to_vec());
        self.semantic.learn(latent.to_vec(), 0.75);
        let raw = self.episodic.synchrony(latent);
        self.alpha_raw = raw;

        // Inertial smoothing: clamp the step change
        let lower = (self.alpha_sync - ALPHA_DECAY).max(ALPHA_FLOOR);
        let upper = self.alpha_sync + ALPHA_DECAY;
        self.alpha_sync = raw.clamp(lower, upper);
    }

    /// Retrieve the top-k semantically weighted prototypes.
    pub fn retrieve(&self, query: &[f64], k: usize) -> Vec<(Vec<f64>, f64)> {
        self.semantic.recall(query, k)
    }
}
