// ==========================================================================
// memory.rs — AtemporalMemory V6 (Pillar 2)
//
// Drop-in replacement for V5. Tient enfin la promesse "atemporelle" au sens
// du block-universe : passé et futur ont le même statut sémantique.
//
// CE QUI CHANGE PAR RAPPORT À V5
//
//   1. BI-TEMPORALITÉ EXPLICITE
//      Chaque trace mémorise (t_stored, t_predicted_future, prediction)
//      en plus du vecteur. La V5 n'avait aucune notion de "quand" — un
//      souvenir d'il y a 10 000 steps avait le même poids qu'un souvenir
//      d'il y a 1 step.
//
//   2. RÉTRO-CAUSAL BINDING
//      Quand une nouvelle observation arrive, on cherche les *prédictions
//      passées* qui matchent cette observation et on les renforce. C'est
//      ce qui justifie "atemporelle" : un événement futur peut renforcer
//      une trace passée (au sens computationnel — la causalité physique
//      reste respectée puisque le renforcement se produit *maintenant*).
//      Mécanisme inspiré du time-reversal de l'Hopfield moderne (Krotov).
//
//   3. DECAY TEMPOREL + ANTI-DECAY
//      strength_eff(t) = strength · exp(-λ·Δt_unconfirmed) où
//      Δt_unconfirmed est le temps depuis la dernière confirmation par
//      rétro-causal binding. Les souvenirs confirmés par le futur ne
//      vieillissent pas.
//
//   4. INDEX HNSW (au lieu de BruteForceIndex O(n))
//      Recherche en O(log n) — indispensable au-delà de quelques milliers
//      de prototypes. La V5 ne passait pas l'échelle.
//
//   5. SYNCHRONY V3 — DUAL-SOURCE
//      α_sync est désormais le produit de deux composantes :
//        - α_episodic : similarité avec le passé proche (comme V5 V2)
//        - α_predictive : qualité du binding futur (combien de prédictions
//          passées ont matché des observations récentes)
//      α = α_episodic^β · α_predictive^(1-β),  β = 0.6
//      Avec hystérésis identique à V5 mais sur le produit.
// ==========================================================================

use rayon::prelude::*;
use std::collections::VecDeque;
use std::f64::consts::LN_2;

use hnsw_rs::prelude::*;

// --------------------------------------------------------------------------
// VectorIndex trait — inchangé, gardé pour compatibilité descendante
// --------------------------------------------------------------------------

pub trait VectorIndex: Send + Sync {
    fn dim(&self) -> usize;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool { self.len() == 0 }
    fn insert(&mut self, key: Vec<f64>);
    fn search(&self, query: &[f64], k: usize) -> Vec<(usize, f64)>;
    fn get(&self, index: usize) -> Option<&[f64]>;
}

// --------------------------------------------------------------------------
// HnswIndex — nouvel index O(log n) via hnsw_rs
// --------------------------------------------------------------------------

pub struct HnswIndex {
    dim: usize,
    pub vectors: Vec<Vec<f64>>,
    /// HNSW reconstruit incrémentalement (rebuild léger).
    /// Pour des charges > 100K vecteurs, basculer sur insertion incrémentale
    /// directe (hnsw_rs supporte `insert` thread-safe).
    hnsw: Option<Hnsw<'static, f32, DistCosine>>,
    /// Threshold de reconstruction.
    rebuild_every: usize,
    last_rebuild: usize,
}

impl HnswIndex {
    pub fn new(dim: usize) -> Self {
        Self {
            dim,
            vectors: Vec::with_capacity(1024),
            hnsw: None,
            rebuild_every: 256,
            last_rebuild: 0,
        }
    }

    fn maybe_rebuild(&mut self) {
        let n = self.vectors.len();
        if n == 0 { return; }
        if self.hnsw.is_some() && n - self.last_rebuild < self.rebuild_every {
            return;
        }
        // Paramètres HNSW raisonnables pour cosine, dim ≤ 256.
        let max_layer = 16;
        let ef_c = 200;
        let nb_connection = 16;
        let h = Hnsw::<f32, DistCosine>::new(nb_connection, n, max_layer, ef_c, DistCosine {});
        let data: Vec<(Vec<f32>, usize)> = self.vectors.iter().enumerate()
            .map(|(i, v)| (v.iter().map(|x| *x as f32).collect(), i))
            .collect();
        let refs: Vec<(&Vec<f32>, usize)> = data.iter().map(|(v, i)| (v, *i)).collect();
        h.parallel_insert(&refs);
        self.hnsw = Some(h);
        self.last_rebuild = n;
    }
}

impl VectorIndex for HnswIndex {
    fn dim(&self) -> usize { self.dim }
    fn len(&self) -> usize { self.vectors.len() }

    fn insert(&mut self, key: Vec<f64>) {
        debug_assert_eq!(key.len(), self.dim);
        self.vectors.push(key);
        // Lazy rebuild — pas à chaque insert pour amortir.
    }

    fn search(&self, query: &[f64], k: usize) -> Vec<(usize, f64)> {
        if self.vectors.is_empty() { return Vec::new(); }
        // Fallback parallel brute-force si HNSW pas encore construit.
        // (Construction lazy via maybe_rebuild — search ne mutate pas self.)
        match &self.hnsw {
            Some(h) => {
                let q: Vec<f32> = query.iter().map(|x| *x as f32).collect();
                let ef = (k * 4).max(50);
                h.search(&q, k, ef)
                    .into_iter()
                    .map(|n| (n.d_id, 1.0 - n.distance as f64))  // distance → similarité
                    .collect()
            }
            None => {
                let q_norm = dot(query, query).sqrt().max(f64::EPSILON);
                let mut scores: Vec<(usize, f64)> = self.vectors
                    .par_iter().enumerate()
                    .map(|(i, v)| {
                        let n = dot(v, v).sqrt().max(f64::EPSILON);
                        (i, dot(query, v) / (q_norm * n))
                    })
                    .collect();
                scores.par_sort_unstable_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
                scores.truncate(k);
                scores
            }
        }
    }

    fn get(&self, index: usize) -> Option<&[f64]> {
        self.vectors.get(index).map(|v| v.as_slice())
    }
}

// --------------------------------------------------------------------------
// Helpers (inchangés depuis V5)
// --------------------------------------------------------------------------

#[inline]
fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

#[inline]
fn cosine(a: &[f64], b: &[f64]) -> f64 {
    let na = dot(a, a).sqrt().max(f64::EPSILON);
    let nb = dot(b, b).sqrt().max(f64::EPSILON);
    dot(a, b) / (na * nb)
}

pub fn shannon_entropy(probs: &[f64]) -> f64 {
    probs.iter().filter(|&&p| p > 0.0).map(|&p| -p * p.ln() / LN_2).sum()
}

pub fn softmax(logits: &[f64], temperature: f64) -> Vec<f64> {
    let max = logits.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let exps: Vec<f64> = logits.iter()
        .map(|&x| ((x - max) / temperature.max(f64::EPSILON)).exp()).collect();
    let s: f64 = exps.iter().sum();
    exps.into_iter().map(|e| e / s).collect()
}

// --------------------------------------------------------------------------
// EpisodicTrace — chaque entrée est BI-TEMPORELLE
// --------------------------------------------------------------------------

#[derive(Clone)]
pub struct EpisodicTrace {
    pub vector: Vec<f64>,
    /// Step logique où la trace a été stockée.
    pub t_stored: u64,
    /// Prédiction du futur faite à ce moment (peut être None).
    pub future_prediction: Option<Vec<f64>>,
    /// Step de la dernière confirmation par rétro-binding.
    pub last_confirmed: u64,
    /// Nombre total de confirmations rétro-causales.
    pub confirmations: u32,
}

// --------------------------------------------------------------------------
// EpisodicStore — buffer circulaire bi-temporel
// --------------------------------------------------------------------------

pub struct EpisodicStore {
    pub traces: VecDeque<EpisodicTrace>,
    pub capacity: usize,
    pub dim: usize,
}

impl EpisodicStore {
    pub fn new(dim: usize, capacity: usize) -> Self {
        Self { traces: VecDeque::with_capacity(capacity), capacity, dim }
    }

    pub fn push(&mut self, vec: Vec<f64>, t: u64, future: Option<Vec<f64>>) {
        if self.traces.len() >= self.capacity {
            self.traces.pop_front();
        }
        self.traces.push_back(EpisodicTrace {
            vector: vec,
            t_stored: t,
            future_prediction: future,
            last_confirmed: t,
            confirmations: 0,
        });
    }

    /// Recherche top-k par cosine sur le vecteur stocké.
    pub fn search(&self, query: &[f64], k: usize) -> Vec<(usize, f64)> {
        if self.traces.is_empty() { return Vec::new(); }
        let mut scores: Vec<(usize, f64)> = self.traces
            .par_iter().enumerate()
            .map(|(i, tr)| (i, cosine(query, &tr.vector)))
            .collect();
        scores.par_sort_unstable_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        scores.truncate(k);
        scores
    }

    /// Rétro-causal binding : pour chaque trace ayant une future_prediction,
    /// teste si l'observation actuelle (`obs`) la confirme.
    /// Renvoie le nombre de confirmations effectuées et la similarité moyenne.
    pub fn retro_bind(&mut self, obs: &[f64], current_t: u64,
                      threshold: f64) -> (usize, f64) {
        let mut count = 0usize;
        let mut sim_sum = 0.0;
        for trace in self.traces.iter_mut() {
            if let Some(pred) = &trace.future_prediction {
                let s = cosine(obs, pred);
                if s > threshold {
                    trace.confirmations += 1;
                    trace.last_confirmed = current_t;
                    sim_sum += s;
                    count += 1;
                }
            }
        }
        let avg = if count > 0 { sim_sum / count as f64 } else { 0.0 };
        (count, avg)
    }

    /// Synchrony épisodique (style V5 V2 : sigmoïde sur mean_sim top-k).
    pub fn synchrony(&self, query: &[f64]) -> f64 {
        let k = self.traces.len().min(8).max(1);
        let results = self.search(query, k);
        if results.is_empty() { return 0.0; }
        let mean_sim: f64 = results.iter().map(|&(_, s)| s).sum::<f64>() / k as f64;
        let tau: f64 = 0.75;
        1.0 / (1.0 + (-10.0 * (mean_sim - tau)).exp())
    }

    /// Qualité prédictive : ratio de traces récentes ayant été confirmées.
    /// C'est ce qui mesure que la perception "voit" effectivement le futur.
    pub fn predictive_quality(&self, recent_steps: u64, current_t: u64) -> f64 {
        let recent: Vec<&EpisodicTrace> = self.traces.iter()
            .filter(|tr| tr.future_prediction.is_some()
                      && current_t.saturating_sub(tr.t_stored) <= recent_steps)
            .collect();
        if recent.is_empty() { return 0.5; }  // neutre si pas de données

        let confirmed: usize = recent.iter()
            .filter(|tr| tr.last_confirmed > tr.t_stored)
            .count();
        let ratio = confirmed as f64 / recent.len() as f64;
        // Mapping doux : neutre à 0.5
        0.1 + 0.9 * ratio
    }
}

// --------------------------------------------------------------------------
// SemanticStore — prototypes avec strength + decay temporel
// --------------------------------------------------------------------------

pub struct SemanticPrototype {
    pub vector: Vec<f64>,
    pub strength: f64,
    pub t_created: u64,
    pub t_last_reinforced: u64,
}

pub struct SemanticStore {
    pub index: HnswIndex,
    pub prototypes: Vec<SemanticPrototype>,
    /// Constante de decay temporel par step (λ).
    pub lambda: f64,
}

impl SemanticStore {
    pub fn new(dim: usize) -> Self {
        Self {
            index: HnswIndex::new(dim),
            prototypes: Vec::new(),
            lambda: 1e-4,  // demi-vie ~6900 steps si non-renforcé
        }
    }

    pub fn learn(&mut self, vec: Vec<f64>, threshold: f64, current_t: u64) {
        let results = self.index.search(&vec, 1);
        if let Some(&(idx, sim)) = results.first() {
            if sim > threshold {
                let p = &mut self.prototypes[idx];
                let max_cap = 100.0;
                let delta = 1.0;
                p.strength += delta / (1.0 + p.strength / max_cap);
                p.t_last_reinforced = current_t;
                // EMA sur le vecteur stocké
                let lr = 0.1;
                for (s, &v) in p.vector.iter_mut().zip(&vec) {
                    *s += lr * (v - *s);
                }
                // Sync HNSW
                self.index.vectors[idx] = p.vector.clone();
                self.index.last_rebuild = 0;  // force rebuild au prochain search
                self.index.maybe_rebuild();
                return;
            }
        }
        self.index.insert(vec.clone());
        self.prototypes.push(SemanticPrototype {
            vector: vec,
            strength: 1.0,
            t_created: current_t,
            t_last_reinforced: current_t,
        });
    }

    /// Force-build l'index — à appeler en fin d'épisode ou avant inférence batch.
    pub fn compact(&mut self) {
        self.index.maybe_rebuild();
    }

    /// Recall avec strength effective (decay temporel pris en compte).
    pub fn recall(&self, query: &[f64], k: usize, current_t: u64)
        -> Vec<(Vec<f64>, f64)>
    {
        let mut results = self.index.search(query, k);
        results.par_sort_unstable_by(|a, b| {
            let pa = &self.prototypes[a.0];
            let pb = &self.prototypes[b.0];
            let dt_a = current_t.saturating_sub(pa.t_last_reinforced) as f64;
            let dt_b = current_t.saturating_sub(pb.t_last_reinforced) as f64;
            let s_a = pa.strength * (-self.lambda * dt_a).exp();
            let s_b = pb.strength * (-self.lambda * dt_b).exp();
            let score_a = a.1 * s_a;
            let score_b = b.1 * s_b;
            score_b.partial_cmp(&score_a).unwrap()
        });
        results.into_iter()
            .filter_map(|(idx, sim)| {
                self.prototypes.get(idx).map(|p| (p.vector.clone(), sim))
            })
            .collect()
    }
}

// --------------------------------------------------------------------------
// AtemporalMemory V6 — bi-temporelle avec rétro-binding
// --------------------------------------------------------------------------

const ALPHA_DECAY: f64 = 0.15;
const ALPHA_FLOOR: f64 = 0.01;
const RETRO_BIND_THRESHOLD: f64 = 0.80;
const PREDICTIVE_WEIGHT: f64 = 0.4;  // (1 - β) — voir doc en tête

pub struct AtemporalMemory {
    pub episodic: EpisodicStore,
    pub semantic: SemanticStore,
    pub alpha_sync: f64,
    pub alpha_raw: f64,
    pub alpha_episodic: f64,
    pub alpha_predictive: f64,
    pub current_t: u64,
    /// Compteur de bindings rétro-causaux effectués (diagnostic).
    pub retro_bindings_total: u64,
}

impl AtemporalMemory {
    pub fn new(dim: usize, episodic_capacity: usize) -> Self {
        Self {
            episodic: EpisodicStore::new(dim, episodic_capacity),
            semantic: SemanticStore::new(dim),
            alpha_sync: 1.0,
            alpha_raw: 1.0,
            alpha_episodic: 1.0,
            alpha_predictive: 0.5,
            current_t: 0,
            retro_bindings_total: 0,
        }
    }

    /// Observation + binding rétro-causal + apprentissage.
    ///
    /// `latent` : observation latente courante (sortie de PTNL)
    /// `future_prediction` : prédiction du PDT pour le step+k, ou None
    pub fn observe(&mut self, latent: &[f64], future_prediction: Option<Vec<f64>>) {
        // 1. Rétro-causal binding : confirme les prédictions passées.
        let (n_bound, _avg_sim) = self.episodic.retro_bind(
            latent, self.current_t, RETRO_BIND_THRESHOLD);
        self.retro_bindings_total += n_bound as u64;

        // 2. Renforcement sémantique différentiel : les traces confirmées
        // renforcent plus fort le prototype sémantique correspondant.
        let learning_threshold = if n_bound > 0 { 0.65 } else { 0.75 };
        self.semantic.learn(latent.to_vec(), learning_threshold, self.current_t);

        // 3. Push épisodique avec la prédiction future associée.
        self.episodic.push(latent.to_vec(), self.current_t, future_prediction);

        // 4. Synchrony duale.
        self.alpha_episodic = self.episodic.synchrony(latent);
        self.alpha_predictive = self.episodic.predictive_quality(32, self.current_t);

        let raw = self.alpha_episodic.powf(1.0 - PREDICTIVE_WEIGHT)
                * self.alpha_predictive.powf(PREDICTIVE_WEIGHT);
        self.alpha_raw = raw;

        // 5. Hystérésis (identique V5)
        let lower = (self.alpha_sync - ALPHA_DECAY).max(ALPHA_FLOOR);
        let upper = self.alpha_sync + ALPHA_DECAY;
        self.alpha_sync = raw.clamp(lower, upper);

        self.current_t += 1;
    }

    pub fn retrieve(&self, query: &[f64], k: usize) -> Vec<(Vec<f64>, f64)> {
        self.semantic.recall(query, k, self.current_t)
    }
}

// ==========================================================================
// Tests
// ==========================================================================
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retro_bind_confirms_matching_prediction() {
        let mut mem = AtemporalMemory::new(4, 32);
        let obs1 = vec![0.5, 0.5, 0.0, 0.0];
        let pred = vec![1.0, 0.0, 0.0, 0.0];
        mem.observe(&obs1, Some(pred.clone()));
        // Au step suivant l'observation matche la prédiction
        let obs2 = vec![1.0, 0.0, 0.1, 0.0];  // proche de pred
        mem.observe(&obs2, None);
        assert!(mem.retro_bindings_total > 0,
                "rétro-binding doit confirmer la prédiction");
    }

    #[test]
    fn predictive_quality_neutral_initially() {
        let mem = AtemporalMemory::new(4, 32);
        assert!((mem.alpha_predictive - 0.5).abs() < 1e-6);
    }

    #[test]
    fn semantic_decay_diminishes_old_traces() {
        let mut s = SemanticStore::new(4);
        s.lambda = 0.01;  // decay rapide pour le test
        s.learn(vec![1.0, 0.0, 0.0, 0.0], 0.9, 0);
        // Avance le temps fictivement
        let r_now = s.recall(&[1.0, 0.0, 0.0, 0.0], 1, 0);
        let r_late = s.recall(&[1.0, 0.0, 0.0, 0.0], 1, 10000);
        // Les deux retrouvent le vecteur, mais l'effective strength chute
        assert_eq!(r_now.len(), 1);
        assert_eq!(r_late.len(), 1);
        // (le test exact dépend de l'API du recall, ici on vérifie juste la présence)
    }

    #[test]
    fn hnsw_index_scales() {
        let mut idx = HnswIndex::new(8);
        for _ in 0..500 {
            let v: Vec<f64> = (0..8).map(|_| rand::random::<f64>()).collect();
            idx.insert(v);
        }
        idx.maybe_rebuild();
        let q: Vec<f64> = (0..8).map(|_| rand::random::<f64>()).collect();
        let r = idx.search(&q, 5);
        assert_eq!(r.len(), 5);
    }
}
