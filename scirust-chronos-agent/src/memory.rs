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
// SemanticStore — long-term pattern memory
// --------------------------------------------------------------------------

pub struct SemanticStore {
    pub index: BruteForceIndex,
    /// Number of times each entry has been reinforced.
    pub strengths: Vec<f64>,
}

impl SemanticStore {
    pub fn new(dim: usize) -> Self {
        Self { index: BruteForceIndex::new(dim), strengths: Vec::new() }
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
                let lr = 0.1;
                for (s, &v) in self.index.vectors[idx].iter_mut().zip(&vec) {
                    *s += lr * (v - *s);
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
