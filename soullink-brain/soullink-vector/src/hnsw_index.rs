// =============================================================================
// soullink-vector/src/hnsw_index.rs
// Index HNSW — hnsw_rs 0.3.4, API vérifiée sur docs.rs + source GitHub
// =============================================================================
//
// API réelle hnsw_rs 0.3.4 :
//   Hnsw<'b, T, D>::new(max_nb_connection, max_elements, max_layer, ef_construction, dist_fn)
//   hnsw.insert((&[T], data_id))              — insère un vecteur (slice + id)
//   hnsw.parallel_insert(&[(&Vec<T>, usize)])  — insertion batch parallèle
//   hnsw.search(&[T], knbn, ef) -> Vec<Neighbour>
//   hnsw.parallel_search(&[Vec<T>], knbn, ef) -> Vec<Vec<Neighbour>>
//   hnsw.set_searching_mode(&mut self, flag: bool)
//   Neighbour { d_id: DataId, distance: f32, p_id: PointId }

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use tracing::{info, instrument, warn};

use hnsw_rs::prelude::{DistCosine, Hnsw, Neighbour};

// ─────────────────────────────────────────────────────────────────────────────
// ERREURS
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum VectorError {
    #[error("Dimension mismatch: expected {expected}, got {got}")]
    DimensionMismatch { expected: usize, got: usize },

    #[error("Index is empty")]
    EmptyIndex,

    #[error("top_k must be >= 1")]
    InvalidTopK,
}

pub type Result<T> = std::result::Result<T, VectorError>;

// ─────────────────────────────────────────────────────────────────────────────
// PARAMÈTRES
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HnswParams {
    /// Connectivité par couche (M). Standard : 16.
    pub max_nb_connection: usize,
    /// Candidats à la construction. Standard : 200–400.
    pub ef_construction: usize,
    /// Candidats à la recherche. Standard : 64.
    pub ef_search: usize,
    /// Couches max. Doit être ≤ 16 (contrainte hnsw_rs NB_LAYER_MAX).
    pub nb_layer: usize,
}

impl Default for HnswParams {
    fn default() -> Self {
        Self {
            max_nb_connection: 16,
            ef_construction: 200,
            ef_search: 64,
            nb_layer: 16,
        }
    }
}

impl HnswParams {
    pub fn high_recall() -> Self {
        Self {
            max_nb_connection: 32,
            ef_construction: 400,
            ef_search: 200,
            nb_layer: 16,
        }
    }
    pub fn low_latency() -> Self {
        Self {
            max_nb_connection: 8,
            ef_construction: 100,
            ef_search: 32,
            nb_layer: 16,
        }
    }

    pub fn estimated_ram_gb(&self, n: usize, dim: usize) -> f64 {
        let vectors = n * dim * 4;
        let graph = n * self.max_nb_connection * 8 * 2;
        (vectors + graph) as f64 / 1e9
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// RÉSULTAT DE RECHERCHE
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub label: String,
    /// Distance cosine hnsw_rs ∈ [0.0, 2.0]
    pub distance: f32,
    /// Similarité [0.0, 1.0] = 1 − distance/2
    pub score: f32,
}

// ─────────────────────────────────────────────────────────────────────────────
// INDEX HNSW
// ─────────────────────────────────────────────────────────────────────────────

pub struct HnswIndex {
    /// Hnsw<'static, f32, DistCosine> sous Mutex.
    /// 'static est valide car hnsw_rs copie les données à l'insertion.
    inner: Mutex<Hnsw<'static, f32, DistCosine>>,
    /// DataId → label (résolution des résultats de search)
    id_to_label: Mutex<Vec<String>>,
    /// label → DataId (dédup à l'insert)
    label_to_id: Mutex<HashMap<String, usize>>,
    params: HnswParams,
    dim: usize,
    count: AtomicUsize,
    /// true une fois set_searching_mode(true) appelé
    search_mode: AtomicBool,
}

impl HnswIndex {
    pub fn new(dim: usize, params: HnswParams, initial_capacity: usize) -> Arc<Self> {
        let hnsw = Hnsw::<f32, DistCosine>::new(
            params.max_nb_connection,
            initial_capacity,
            params.nb_layer,
            params.ef_construction,
            DistCosine {},
        );
        info!(
            "HnswIndex: dim={} M={} ef_c={} ef_s={} cap={} ram_est={:.2}GB",
            dim,
            params.max_nb_connection,
            params.ef_construction,
            params.ef_search,
            initial_capacity,
            params.estimated_ram_gb(initial_capacity, dim)
        );
        Arc::new(Self {
            inner: Mutex::new(hnsw),
            id_to_label: Mutex::new(Vec::with_capacity(initial_capacity)),
            label_to_id: Mutex::new(HashMap::with_capacity(initial_capacity)),
            params,
            dim,
            count: AtomicUsize::new(0),
            search_mode: AtomicBool::new(false),
        })
    }

    /// Insère un vecteur. Idempotent : si `label` existe déjà, no-op.
    pub fn insert(&self, label: String, vector: &[f32]) -> Result<usize> {
        if vector.len() != self.dim {
            return Err(VectorError::DimensionMismatch {
                expected: self.dim,
                got: vector.len(),
            });
        }

        // Dédup rapide
        {
            let idx = self.label_to_id.lock();
            if let Some(&id) = idx.get(&label) {
                return Ok(id);
            }
        }

        let id = self.count.fetch_add(1, Ordering::Relaxed);

        {
            let mut hnsw = self.inner.lock();
            if self.search_mode.load(Ordering::Acquire) {
                hnsw.set_searching_mode(false);
                self.search_mode.store(false, Ordering::Release);
                warn!("HnswIndex: switched back to insert mode for new entry");
            }
            hnsw.insert((vector, id));
        }

        // Mise à jour des maps
        {
            let mut labels = self.id_to_label.lock();
            let mut index = self.label_to_id.lock();
            if labels.len() <= id {
                labels.resize(id + 1, String::new());
            }
            labels[id] = label.clone();
            index.insert(label, id);
        }

        Ok(id)
    }

    /// Insertion batch via Rayon interne de hnsw_rs.
    pub fn insert_batch(&self, items: Vec<(String, Vec<f32>)>) -> Result<usize> {
        let mut to_insert: Vec<(usize, String, Vec<f32>)> = Vec::new();
        {
            let mut idx = self.label_to_id.lock();
            for (label, vec) in items {
                if vec.len() != self.dim {
                    return Err(VectorError::DimensionMismatch {
                        expected: self.dim,
                        got: vec.len(),
                    });
                }
                if idx.contains_key(&label) {
                    continue;
                }
                let id = self.count.fetch_add(1, Ordering::Relaxed);
                idx.insert(label.clone(), id);
                to_insert.push((id, label, vec));
            }
        }

        if to_insert.is_empty() {
            return Ok(0);
        }

        // hnsw_rs parallel_insert attend &[(&Vec<f32>, DataId)]
        let data_refs: Vec<(Vec<f32>, usize)> = to_insert
            .iter()
            .map(|(id, _, vec)| (vec.clone(), *id))
            .collect();
        let data_refs_ref: Vec<(&Vec<f32>, usize)> =
            data_refs.iter().map(|(v, id)| (v, *id)).collect();

        {
            let mut hnsw = self.inner.lock();
            if self.search_mode.load(Ordering::Acquire) {
                hnsw.set_searching_mode(false);
                self.search_mode.store(false, Ordering::Release);
            }
            hnsw.parallel_insert(&data_refs_ref);
        }

        // Mise à jour id_to_label
        {
            let mut labels = self.id_to_label.lock();
            for (id, label, _) in &to_insert {
                if labels.len() <= *id {
                    labels.resize(*id + 1, String::new());
                }
                labels[*id] = label.clone();
            }
        }

        let n = to_insert.len();
        info!(
            "insert_batch: {} vectors indexed (total: {})",
            n,
            self.count.load(Ordering::Relaxed)
        );
        Ok(n)
    }

    /// Passe l'index en mode recherche optimisé.
    pub fn seal_for_search(&self) {
        let mut hnsw = self.inner.lock();
        hnsw.set_searching_mode(true);
        self.search_mode.store(true, Ordering::Release);
        info!("HnswIndex: sealed for search (set_searching_mode=true)");
    }

    /// Recherche les `top_k` plus proches voisins.
    #[instrument(skip(self, query), fields(n = self.count.load(Ordering::Relaxed)))]
    pub fn search(&self, query: &[f32], top_k: usize) -> Result<Vec<SearchResult>> {
        if top_k == 0 {
            return Err(VectorError::InvalidTopK);
        }
        if self.count.load(Ordering::Relaxed) == 0 {
            return Err(VectorError::EmptyIndex);
        }
        if query.len() != self.dim {
            return Err(VectorError::DimensionMismatch {
                expected: self.dim,
                got: query.len(),
            });
        }

        let ef = self.params.ef_search.max(top_k);
        let neighbours: Vec<Neighbour> = self.inner.lock().search(query, top_k, ef);

        let labels = self.id_to_label.lock();
        let results = neighbours
            .iter()
            .filter_map(|n| {
                labels.get(n.d_id).filter(|l| !l.is_empty()).map(|label| {
                    let dist = n.distance;
                    let score = (1.0 - dist / 2.0).clamp(0.0, 1.0);
                    SearchResult {
                        label: label.clone(),
                        distance: dist,
                        score,
                    }
                })
            })
            .collect();

        Ok(results)
    }

    /// Recherche batch parallèle.
    pub fn parallel_search(
        &self,
        queries: &[Vec<f32>],
        top_k: usize,
    ) -> Result<Vec<Vec<SearchResult>>> {
        if self.count.load(Ordering::Relaxed) == 0 {
            return Err(VectorError::EmptyIndex);
        }
        let ef = self.params.ef_search.max(top_k);
        let results = self.inner.lock().parallel_search(queries, top_k, ef);
        let labels = self.id_to_label.lock();

        Ok(results
            .iter()
            .map(|neighbours| {
                neighbours
                    .iter()
                    .filter_map(|n| {
                        labels.get(n.d_id).filter(|l| !l.is_empty()).map(|label| {
                            let dist = n.distance;
                            let score = (1.0 - dist / 2.0).clamp(0.0, 1.0);
                            SearchResult {
                                label: label.clone(),
                                distance: dist,
                                score,
                            }
                        })
                    })
                    .collect()
            })
            .collect())
    }

    // ── Métriques ──────────────────────────────────────────────────────────────

    pub fn len(&self) -> usize {
        self.count.load(Ordering::Relaxed)
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn dim(&self) -> usize {
        self.dim
    }

    pub fn status(&self) -> serde_json::Value {
        serde_json::json!({
            "component":    "HnswIndex",
            "indexed":      self.len(),
            "dim":          self.dim,
            "M":            self.params.max_nb_connection,
            "ef_search":    self.params.ef_search,
            "search_mode":  self.search_mode.load(Ordering::Relaxed),
            "ram_est_gb":   self.params.estimated_ram_gb(self.len(), self.dim),
        })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// RECHERCHE HYBRIDE : brute-force < threshold, HNSW au-dessus
// ─────────────────────────────────────────────────────────────────────────────

pub struct HybridSearch {
    threshold: usize,
    bf_store: Mutex<Vec<(String, Vec<f32>)>>,
    hnsw: Arc<HnswIndex>,
    hnsw_active: AtomicBool,
}

impl HybridSearch {
    pub fn new(dim: usize, params: HnswParams) -> Arc<Self> {
        let cap = 10_000 * 2;
        Arc::new(Self {
            threshold: 10_000,
            bf_store: Mutex::new(Vec::new()),
            hnsw: HnswIndex::new(dim, params, cap),
            hnsw_active: AtomicBool::new(false),
        })
    }

    pub fn insert(&self, label: String, vector: Vec<f32>) -> Result<()> {
        if self.hnsw_active.load(Ordering::Acquire) {
            self.hnsw.insert(label, &vector)?;
            return Ok(());
        }

        let n = {
            let mut store = self.bf_store.lock();
            store.push((label.clone(), vector.clone()));
            store.len()
        };

        if n >= self.threshold {
            self.migrate()?;
        }
        Ok(())
    }

    fn migrate(&self) -> Result<()> {
        if self
            .hnsw_active
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Ok(());
        }

        let items: Vec<(String, Vec<f32>)> = self.bf_store.lock().drain(..).collect();
        let n = items.len();
        info!("HybridSearch: migrating {} vectors to HNSW", n);
        self.hnsw.insert_batch(items)?;
        self.hnsw.seal_for_search();
        info!("HybridSearch: migration complete");
        Ok(())
    }

    pub fn search(&self, query: &[f32], top_k: usize) -> Result<Vec<SearchResult>> {
        if self.hnsw_active.load(Ordering::Acquire) {
            return self.hnsw.search(query, top_k);
        }
        let store = self.bf_store.lock();
        if store.is_empty() {
            return Err(VectorError::EmptyIndex);
        }

        let mut scores: Vec<(String, f32)> = store
            .iter()
            .map(|(label, vec)| (label.clone(), cosine(query, vec)))
            .collect();
        scores.sort_unstable_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scores.truncate(top_k);

        Ok(scores
            .into_iter()
            .map(|(label, score)| SearchResult {
                label,
                distance: (1.0 - score) * 2.0,
                score,
            })
            .collect())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn len(&self) -> usize {
        if self.hnsw_active.load(Ordering::Acquire) {
            self.hnsw.len()
        } else {
            self.bf_store.lock().len()
        }
    }

    pub fn is_hnsw_active(&self) -> bool {
        self.hnsw_active.load(Ordering::Acquire)
    }
}

/// Cosine scalaire — LLVM vectorise en AVX2 avec -C target-cpu=broadwell.
#[inline(always)]
fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na * nb == 0.0 {
        0.0
    } else {
        dot / (na * nb)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// VectorStore — compat wrapper for soullink-memory
// ─────────────────────────────────────────────────────────────────────────────

#[allow(dead_code)]
pub struct VectorStore {
    inner: Arc<HybridSearch>,
    dim: usize,
}

impl VectorStore {
    pub fn new(params: HnswParams) -> Self {
        let dim = 384;
        Self {
            inner: HybridSearch::new(dim, params),
            dim,
        }
    }

    pub fn new_default() -> Self {
        Self::new(HnswParams::default())
    }

    pub fn insert(&self, label: String, vector: Vec<f32>) -> Result<()> {
        self.inner.insert(label, vector)?;
        Ok(())
    }

    pub fn search(&self, query: &[f32], top_k: usize) -> Vec<SearchResult> {
        self.inner.search(query, top_k).unwrap_or_default()
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.len() == 0
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// TESTS
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn pvec(dim: usize, seed: u64) -> Vec<f32> {
        let mut s = seed;
        let mut v: Vec<f32> = (0..dim)
            .map(|_| {
                s = s
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                (s >> 33) as f32 / u32::MAX as f32 * 2.0 - 1.0
            })
            .collect();
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            v.iter_mut().for_each(|x| *x /= norm);
        }
        v
    }

    #[test]
    fn insert_and_retrieve() {
        let idx = HnswIndex::new(16, HnswParams::default(), 100);
        let v = pvec(16, 1);
        let id = idx.insert("alpha".into(), &v).unwrap();
        assert_eq!(id, 0);
        assert_eq!(idx.len(), 1);
    }

    #[test]
    fn search_returns_self_as_nearest() {
        let idx = HnswIndex::new(16, HnswParams::default(), 100);
        let v1 = pvec(16, 1);
        let v2 = pvec(16, 2);
        let v3 = pvec(16, 3);
        idx.insert("alpha".into(), &v1).unwrap();
        idx.insert("beta".into(), &v2).unwrap();
        idx.insert("gamma".into(), &v3).unwrap();

        let results = idx.search(&v1, 1).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].label, "alpha");
        assert!(
            results[0].score > 0.99,
            "expected score≈1.0, got {}",
            results[0].score
        );
    }

    #[test]
    fn dimension_mismatch_is_rejected() {
        let idx = HnswIndex::new(16, HnswParams::default(), 100);
        let err = idx.insert("x".into(), &pvec(32, 1)).unwrap_err();
        assert!(matches!(
            err,
            VectorError::DimensionMismatch {
                expected: 16,
                got: 32
            }
        ));
    }

    #[test]
    fn duplicate_label_is_idempotent() {
        let idx = HnswIndex::new(16, HnswParams::default(), 100);
        let v = pvec(16, 7);
        idx.insert("dup".into(), &v).unwrap();
        idx.insert("dup".into(), &v).unwrap();
        assert_eq!(idx.len(), 1);
    }

    #[test]
    fn empty_index_search_errors() {
        let idx = HnswIndex::new(16, HnswParams::default(), 100);
        let err = idx.search(&pvec(16, 0), 3).unwrap_err();
        assert!(matches!(err, VectorError::EmptyIndex));
    }

    #[test]
    fn top_k_clipped_to_available() {
        let idx = HnswIndex::new(16, HnswParams::default(), 100);
        for i in 0..5u64 {
            idx.insert(format!("c{}", i), &pvec(16, i)).unwrap();
        }
        let r = idx.search(&pvec(16, 99), 10).unwrap();
        assert!(r.len() <= 5, "got {} results for 5 vectors", r.len());
    }

    #[test]
    fn batch_insert_correct_count() {
        let idx = HnswIndex::new(16, HnswParams::default(), 200);
        let items: Vec<(String, Vec<f32>)> = (0..50u64)
            .map(|i| (format!("v{}", i), pvec(16, i)))
            .collect();
        let n = idx.insert_batch(items).unwrap();
        assert_eq!(n, 50);
        assert_eq!(idx.len(), 50);
    }

    #[test]
    fn seal_then_insert_re_opens() {
        let idx = HnswIndex::new(16, HnswParams::default(), 100);
        idx.insert("a".into(), &pvec(16, 1)).unwrap();
        idx.seal_for_search();
        assert!(idx.search_mode.load(Ordering::Relaxed));
        idx.insert("b".into(), &pvec(16, 2)).unwrap();
        assert!(!idx.search_mode.load(Ordering::Relaxed));
        assert_eq!(idx.len(), 2);
    }

    #[test]
    fn hybrid_brute_force_below_threshold() {
        let hs = HybridSearch::new(16, HnswParams::default());
        hs.insert("x".into(), pvec(16, 1)).unwrap();
        hs.insert("y".into(), pvec(16, 2)).unwrap();
        assert!(!hs.is_hnsw_active());
        let r = hs.search(&pvec(16, 1), 1).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].label, "x");
    }

    #[test]
    fn cosine_self_is_one() {
        let v = pvec(384, 42);
        let s = cosine(&v, &v);
        assert!((s - 1.0).abs() < 1e-5, "cosine(v,v)={}", s);
    }

    #[test]
    fn cosine_orthogonal_is_zero() {
        let mut a = vec![0.0f32; 8];
        a[0] = 1.0;
        let mut b = vec![0.0f32; 8];
        b[1] = 1.0;
        assert!((cosine(&a, &b)).abs() < 1e-6);
    }

    #[test]
    fn ram_estimate_10m_sane() {
        let p = HnswParams::default();
        let ram = p.estimated_ram_gb(10_000_000, 384);
        assert!(ram > 10.0 && ram < 128.0, "ram={:.2}GB", ram);
    }
}
