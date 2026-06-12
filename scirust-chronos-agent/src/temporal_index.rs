// ==========================================================================
// temporal_index.rs — Indexation chronologique (V6.3, point 1.A du doc)
//
// Aujourd'hui les requêtes "trouve les souvenirs des N dernières steps"
// nécessitent de balayer tout l'EpisodicStore en O(n). Avec un index
// chronologique on tombe à O(log n + k) où k = nombre de résultats.
//
// IMPLÉMENTATION
//
//   BTreeMap<u64, Vec<usize>> où :
//     - clé   = timestamp (t_stored)
//     - valeur = indices dans l'EpisodicStore.traces
//
//   BTreeMap maintient l'ordre — `range(t_min..t_max)` est O(log n + k)
//   et retourne directement la plage temporelle voulue.
//
// USAGE
//
//   let mut idx = TemporalIndex::new();
//   idx.insert(42, 0);   // trace #0 stockée au step 42
//   idx.insert(43, 1);
//   idx.insert(99, 2);
//
//   // "souvenirs entre step 40 et 50"
//   let indices = idx.range(40..50);   // → [0, 1]
//
//   // "N derniers souvenirs avant le step actuel"
//   let recent = idx.last_n_before(100, 5);
//
// INTÉGRATION DANS EPISODICSTORE
//
//   - À chaque `push()`, indexer le timestamp.
//   - À chaque `prune_low_importance()`, retirer les indices supprimés.
//   - Le rebuild complet après prune est nécessaire (les indices internes
//     changent après suppression). Coût O(n log n) — acceptable car le prune
//     est rare.
// ==========================================================================

use std::collections::BTreeMap;
use std::ops::RangeBounds;

/// Index chronologique des traces. Chaque entrée `(t_stored → indices)`
/// liste tous les indices de traces stockées à ce timestamp (plusieurs
/// peuvent partager le même).
#[derive(Default, Clone)]
pub struct TemporalIndex {
    /// BTreeMap garanti par contrat : itération en ordre de clé croissant,
    /// `range()` en O(log n + k).
    map: BTreeMap<u64, Vec<usize>>,
    /// Compteur d'éléments indexés (peut différer de map.len() car
    /// plusieurs indices par clé sont possibles).
    pub total_entries: usize,
}

impl TemporalIndex {
    pub fn new() -> Self { Self::default() }

    /// Ajoute (timestamp → index). O(log n).
    pub fn insert(&mut self, timestamp: u64, trace_index: usize) {
        self.map.entry(timestamp).or_default().push(trace_index);
        self.total_entries += 1;
    }

    /// Retourne tous les indices de traces dont le timestamp est dans range.
    /// O(log n + k). L'ordre retourné est chronologique croissant.
    pub fn range<R: RangeBounds<u64>>(&self, range: R) -> Vec<usize> {
        self.map.range(range)
            .flat_map(|(_, v)| v.iter().copied())
            .collect()
    }

    /// Les N derniers indices avant `current_t` (exclus).
    /// Retourne les indices dans l'ordre du plus récent au plus ancien.
    pub fn last_n_before(&self, current_t: u64, n: usize) -> Vec<usize> {
        let mut out = Vec::with_capacity(n);
        for (_, indices) in self.map.range(..current_t).rev() {
            for &idx in indices.iter().rev() {
                out.push(idx);
                if out.len() >= n { return out; }
            }
        }
        out
    }

    /// Les indices dont le timestamp est dans la fenêtre [current_t - window, current_t].
    pub fn window_before(&self, current_t: u64, window: u64) -> Vec<usize> {
        let from = current_t.saturating_sub(window);
        self.range(from..=current_t)
    }

    /// Reconstruit complètement l'index à partir d'une liste de (timestamp, index).
    /// Utilisé après un pruning où les indices internes ont changé.
    pub fn rebuild_from<I: IntoIterator<Item = (u64, usize)>>(&mut self, entries: I) {
        self.map.clear();
        self.total_entries = 0;
        for (t, idx) in entries {
            self.insert(t, idx);
        }
    }

    pub fn len(&self) -> usize { self.total_entries }
    pub fn is_empty(&self) -> bool { self.map.is_empty() }
    pub fn clear(&mut self) {
        self.map.clear();
        self.total_entries = 0;
    }

    /// Le timestamp le plus ancien indexé.
    pub fn earliest(&self) -> Option<u64> {
        self.map.keys().next().copied()
    }

    /// Le timestamp le plus récent indexé.
    pub fn latest(&self) -> Option<u64> {
        self.map.keys().next_back().copied()
    }
}

// ==========================================================================
// Tests
// ==========================================================================
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_range() {
        let mut idx = TemporalIndex::new();
        idx.insert(10, 0);
        idx.insert(15, 1);
        idx.insert(20, 2);
        idx.insert(25, 3);
        let r = idx.range(15..=20);
        assert_eq!(r, vec![1, 2]);
    }

    #[test]
    fn last_n_before_returns_recent_first() {
        let mut idx = TemporalIndex::new();
        for t in [10u64, 20, 30, 40, 50] {
            idx.insert(t, t as usize);
        }
        let recent = idx.last_n_before(45, 2);
        // Plus récents avant 45 : 40 puis 30
        assert_eq!(recent, vec![40, 30]);
    }

    #[test]
    fn window_before() {
        let mut idx = TemporalIndex::new();
        for t in [10u64, 20, 30, 40, 50] {
            idx.insert(t, t as usize);
        }
        // Fenêtre [35-15, 35] = [20, 35] → idx 20, 30
        let w = idx.window_before(35, 15);
        assert_eq!(w, vec![20, 30]);
    }

    #[test]
    fn multiple_indices_per_timestamp() {
        let mut idx = TemporalIndex::new();
        idx.insert(42, 0);
        idx.insert(42, 1);
        idx.insert(42, 2);
        assert_eq!(idx.range(42..=42), vec![0, 1, 2]);
        assert_eq!(idx.len(), 3);
    }

    #[test]
    fn rebuild_after_prune() {
        let mut idx = TemporalIndex::new();
        idx.insert(10, 0);
        idx.insert(20, 1);
        idx.insert(30, 2);

        // Simule un prune qui supprime l'index 1 et réindexe : 0, 1 (était 2)
        let new_entries = vec![(10u64, 0), (30, 1)];
        idx.rebuild_from(new_entries);

        assert_eq!(idx.range(0..100), vec![0, 1]);
        assert_eq!(idx.len(), 2);
    }

    #[test]
    fn earliest_and_latest() {
        let mut idx = TemporalIndex::new();
        assert!(idx.earliest().is_none());
        idx.insert(99, 0);
        idx.insert(1, 1);
        idx.insert(50, 2);
        assert_eq!(idx.earliest(), Some(1));
        assert_eq!(idx.latest(), Some(99));
    }
}
