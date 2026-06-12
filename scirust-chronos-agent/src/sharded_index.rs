// ==========================================================================
// sharded_index.rs — Index vectoriel partitionné (V6.5, point 1.C du doc)
//
// "Le stockage vectoriel actuel pourrait à terme être remplacé par un
//  moteur plus scalable (FAISS, Milvus, Qdrant) avec indexation par
//  fragments (sharding). En partitionnant par plage de temps ou par thème,
//  on accélère les recherches de similarité et on permet à la mémoire de
//  grandir sans dégrader les performances."
//
// APPROCHE
//
//   Plutôt que de dépendre immédiatement de FAISS/Qdrant (FFI lourde,
//   déploiement externe), on implémente le SHARDING LOGIQUE en pur Rust.
//   Chaque shard est un HnswIndex indépendant. Un routeur dirige insert
//   et search vers le(s) bon(s) shard(s).
//
//   Le trait ShardRouter permet de brancher n'importe quelle stratégie :
//     - TemporalRouter : shard par plage de timestamp (souvenirs récents
//       vs anciens dans des shards séparés)
//     - HashRouter : shard par hash du vecteur (répartition uniforme)
//     - ThemeRouter : shard par cluster sémantique (souvenirs proches
//       ensemble → recherche plus locale)
//
//   Quand un vrai backend FAISS/Qdrant sera branché, il suffira
//   d'implémenter le trait `ShardBackend` autour du client FAISS — le
//   routeur et l'API publique restent identiques.
//
// GAIN
//
//   - Search peut ne scanner qu'un sous-ensemble de shards (ex: recherche
//     "récente" → seul le shard récent). O(log(n/k)) au lieu de O(log n)
//     mais surtout cache-friendly et parallélisable.
//   - Insert va dans un seul shard → rebuild HNSW local plus rapide.
//   - Croissance : ajouter un shard est O(1), pas de rebuild global.
// ==========================================================================

use rayon::prelude::*;
use crate::memory::{HnswIndex, VectorIndex};

// --------------------------------------------------------------------------
// Stratégie de routage
// --------------------------------------------------------------------------

/// Décide dans quel shard va un vecteur (insert) et quels shards
/// interroger (search).
pub trait ShardRouter: Send + Sync {
    /// Nombre de shards gérés.
    fn num_shards(&self) -> usize;
    /// Shard cible pour un insert, étant donné le vecteur et un timestamp.
    fn route_insert(&self, vector: &[f64], timestamp: u64) -> usize;
    /// Shards à interroger pour une recherche. Par défaut : tous.
    /// Peut être restreint (ex: recherche temporelle → un seul shard).
    fn route_search(&self, query: &[f64], hint: &SearchHint) -> Vec<usize>;
}

/// Indice optionnel passé au routeur pour restreindre la recherche.
#[derive(Default, Clone, Debug)]
pub struct SearchHint {
    /// Si présent, ne chercher que dans la plage temporelle.
    pub time_range: Option<(u64, u64)>,
    /// Si présent, hint thématique (id de cluster).
    pub theme_hint: Option<usize>,
}

// --------------------------------------------------------------------------
// TemporalRouter — shard par plage de timestamp
// --------------------------------------------------------------------------

/// Partitionne par tranches de temps. Shard i = timestamps dans
/// [i·bucket_size, (i+1)·bucket_size). Le dernier shard absorbe tout
/// ce qui dépasse (souvenirs les plus récents).
pub struct TemporalRouter {
    pub num_shards: usize,
    pub bucket_size: u64,
}

impl TemporalRouter {
    pub fn new(num_shards: usize, bucket_size: u64) -> Self {
        Self { num_shards: num_shards.max(1), bucket_size: bucket_size.max(1) }
    }

    fn shard_for_time(&self, t: u64) -> usize {
        ((t / self.bucket_size) as usize).min(self.num_shards - 1)
    }
}

impl ShardRouter for TemporalRouter {
    fn num_shards(&self) -> usize { self.num_shards }

    fn route_insert(&self, _vector: &[f64], timestamp: u64) -> usize {
        self.shard_for_time(timestamp)
    }

    fn route_search(&self, _query: &[f64], hint: &SearchHint) -> Vec<usize> {
        match hint.time_range {
            Some((t_min, t_max)) => {
                let s_min = self.shard_for_time(t_min);
                let s_max = self.shard_for_time(t_max);
                (s_min..=s_max).collect()
            }
            None => (0..self.num_shards).collect(),  // tous
        }
    }
}

// --------------------------------------------------------------------------
// HashRouter — répartition uniforme par hash du vecteur
// --------------------------------------------------------------------------

pub struct HashRouter {
    pub num_shards: usize,
}

impl HashRouter {
    pub fn new(num_shards: usize) -> Self {
        Self { num_shards: num_shards.max(1) }
    }

    fn hash_vector(v: &[f64]) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for x in v { x.to_bits().hash(&mut h); }
        h.finish()
    }
}

impl ShardRouter for HashRouter {
    fn num_shards(&self) -> usize { self.num_shards }

    fn route_insert(&self, vector: &[f64], _timestamp: u64) -> usize {
        (Self::hash_vector(vector) % self.num_shards as u64) as usize
    }

    fn route_search(&self, _query: &[f64], _hint: &SearchHint) -> Vec<usize> {
        // Hash routing : impossible de savoir où est un voisin → tous les shards
        (0..self.num_shards).collect()
    }
}

// --------------------------------------------------------------------------
// ShardedIndex — orchestrateur
// --------------------------------------------------------------------------

pub struct ShardedIndex<R: ShardRouter> {
    shards: Vec<HnswIndex>,
    /// Mapping shard-local index → (global_id) pour reconstruire les
    /// résultats. global_id est attribué à l'insert.
    shard_to_global: Vec<Vec<usize>>,
    router: R,
    dim: usize,
    next_global_id: usize,
}

impl<R: ShardRouter> ShardedIndex<R> {
    pub fn new(dim: usize, router: R) -> Self {
        let n = router.num_shards();
        Self {
            shards: (0..n).map(|_| HnswIndex::new(dim)).collect(),
            shard_to_global: (0..n).map(|_| Vec::new()).collect(),
            router,
            dim,
            next_global_id: 0,
        }
    }

    /// Insère un vecteur, route vers le bon shard. Retourne le global_id.
    pub fn insert(&mut self, vector: Vec<f64>, timestamp: u64) -> usize {
        debug_assert_eq!(vector.len(), self.dim);
        let shard = self.router.route_insert(&vector, timestamp);
        let global_id = self.next_global_id;
        self.next_global_id += 1;
        self.shard_to_global[shard].push(global_id);
        self.shards[shard].insert(vector);
        global_id
    }

    /// Recherche top-k, en interrogeant seulement les shards pertinents
    /// selon le hint. Fusionne les résultats par similarité.
    /// Retourne (global_id, similarité).
    pub fn search(&self, query: &[f64], k: usize, hint: &SearchHint)
        -> Vec<(usize, f64)>
    {
        let target_shards = self.router.route_search(query, hint);

        // Recherche parallèle dans chaque shard pertinent
        let mut merged: Vec<(usize, f64)> = target_shards.par_iter()
            .flat_map(|&shard_idx| {
                let shard = &self.shards[shard_idx];
                let local_results = shard.search(query, k);
                local_results.into_iter()
                    .map(|(local_idx, sim)| {
                        let global = self.shard_to_global[shard_idx]
                            .get(local_idx).copied().unwrap_or(usize::MAX);
                        (global, sim)
                    })
                    .collect::<Vec<_>>()
            })
            .collect();

        // Fusion : tri global par similarité décroissante, top-k
        merged.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        merged.truncate(k);
        merged
    }

    /// Force le rebuild HNSW de tous les shards (à appeler avant inférence batch).
    pub fn compact_all(&mut self) {
        self.shards.par_iter_mut().for_each(|s| s.maybe_rebuild());
    }

    pub fn total_vectors(&self) -> usize {
        self.shards.iter().map(|s| s.len()).sum()
    }

    pub fn shard_sizes(&self) -> Vec<usize> {
        self.shards.iter().map(|s| s.len()).collect()
    }

    pub fn num_shards(&self) -> usize { self.shards.len() }
}

// ==========================================================================
// Tests
// ==========================================================================
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temporal_router_routes_by_time() {
        let r = TemporalRouter::new(4, 100);
        assert_eq!(r.route_insert(&[0.0], 50), 0);    // [0, 100)
        assert_eq!(r.route_insert(&[0.0], 150), 1);   // [100, 200)
        assert_eq!(r.route_insert(&[0.0], 350), 3);   // [300, 400)
        assert_eq!(r.route_insert(&[0.0], 9999), 3);  // déborde → dernier shard
    }

    #[test]
    fn temporal_search_restricts_to_range() {
        let r = TemporalRouter::new(4, 100);
        let hint = SearchHint {
            time_range: Some((150, 250)),  // shards 1 et 2
            theme_hint: None,
        };
        let shards = r.route_search(&[0.0], &hint);
        assert_eq!(shards, vec![1, 2]);

        // Sans hint → tous les shards
        let all = r.route_search(&[0.0], &SearchHint::default());
        assert_eq!(all, vec![0, 1, 2, 3]);
    }

    #[test]
    fn hash_router_distributes() {
        let r = HashRouter::new(4);
        // Vecteurs différents → potentiellement shards différents
        let mut shards_used = std::collections::HashSet::new();
        for i in 0..100 {
            let v = vec![i as f64, (i * 2) as f64];
            shards_used.insert(r.route_insert(&v, 0));
        }
        // Sur 100 vecteurs distincts, on devrait toucher plusieurs shards
        assert!(shards_used.len() > 1, "hash router not distributing");
    }

    #[test]
    fn sharded_index_insert_and_search() {
        let router = TemporalRouter::new(3, 100);
        let mut idx = ShardedIndex::new(4, router);

        // Insère dans différentes tranches temporelles
        idx.insert(vec![1.0, 0.0, 0.0, 0.0], 50);    // shard 0
        idx.insert(vec![0.9, 0.1, 0.0, 0.0], 60);    // shard 0
        idx.insert(vec![0.0, 1.0, 0.0, 0.0], 150);   // shard 1
        idx.insert(vec![0.0, 0.0, 1.0, 0.0], 250);   // shard 2
        idx.compact_all();

        assert_eq!(idx.total_vectors(), 4);

        // Recherche globale : doit trouver le vecteur le plus proche de +x
        let results = idx.search(&[1.0, 0.0, 0.0, 0.0], 2, &SearchHint::default());
        assert!(!results.is_empty());
        // Le meilleur match doit être global_id 0 (le vecteur exact +x)
        assert_eq!(results[0].0, 0);
    }

    #[test]
    fn sharded_search_temporal_restriction() {
        let router = TemporalRouter::new(3, 100);
        let mut idx = ShardedIndex::new(4, router);

        idx.insert(vec![1.0, 0.0, 0.0, 0.0], 50);    // shard 0, global 0
        idx.insert(vec![1.0, 0.0, 0.0, 0.0], 250);   // shard 2, global 1
        idx.compact_all();

        // Recherche restreinte à [200, 300] → ne doit voir que shard 2 (global 1)
        let hint = SearchHint { time_range: Some((200, 300)), theme_hint: None };
        let results = idx.search(&[1.0, 0.0, 0.0, 0.0], 5, &hint);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, 1);  // seul le global_id 1 est dans la plage
    }

    #[test]
    fn shard_sizes_reflect_distribution() {
        let router = TemporalRouter::new(3, 100);
        let mut idx = ShardedIndex::new(2, router);
        idx.insert(vec![0.0, 0.0], 10);   // shard 0
        idx.insert(vec![0.0, 0.0], 20);   // shard 0
        idx.insert(vec![0.0, 0.0], 150);  // shard 1
        let sizes = idx.shard_sizes();
        assert_eq!(sizes[0], 2);
        assert_eq!(sizes[1], 1);
    }
}
