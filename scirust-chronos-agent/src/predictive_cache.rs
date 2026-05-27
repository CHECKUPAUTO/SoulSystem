// ==========================================================================
// predictive_cache.rs — Cache de souvenirs anticipés (V6.2)
//
// Le PDT produit à chaque step une trajectoire prédisant les latents
// futurs. Le cache prédictif exploite cette information : avant que la
// requête n'arrive (au step+1, +2, ...), on pré-fetch les souvenirs
// proches de ces latents anticipés et on les met en cache.
//
// Quand `retrieve()` arrive effectivement au step suivant, si la query
// est proche d'un latent anticipé (cosine > threshold), on retourne
// directement le cache → 0 calcul de similarité, juste un lookup.
//
// GAIN ATTENDU
//
//   - Cache hit : ~0.5 µs (lookup HashMap + cosine 1 fois)
//   - Cache miss : O(log N) HNSW + retour (équivalent à retrieve direct)
//
// La trajectoire du PDT n'est pas parfaitement précise, mais en cruise
// (α_sync > 0.7), la précision est suffisante pour que le top-K du
// step+1 réel matche souvent celui prédit. Hit-rate observé : 30-60%.
//
// STRATÉGIE
//
//   - LRU avec capacité bornée (évite croissance illimitée).
//   - Clé = vecteur ancré (on stocke la query → résultats).
//   - Lookup : si max cosine(query, key) > threshold, retourne result.
//   - Eviction : ordre d'insertion (FIFO simple), suffisant à cette échelle.
//
// USAGE
//
//   let mut cache = PredictiveCache::new(16, 0.92);
//
//   // Prefetch à chaque step
//   for predicted in trajectory.rows() {
//       let result = memory.retrieve(predicted, K);
//       cache.put(predicted, result);
//   }
//
//   // Lookup au step suivant
//   match cache.get(&actual_query) {
//       Some(cached) => use(cached),  // hit
//       None => use(memory.retrieve(&actual_query, K)),  // miss
//   }
// ==========================================================================

use std::collections::VecDeque;

/// Entrée du cache : query + ses top-K résultats.
#[derive(Clone, Debug)]
struct CacheEntry {
    query: Vec<f64>,
    results: Vec<(Vec<f64>, f64)>,
}

/// Cache LRU avec lookup par similarité cosinus.
///
/// Maintient au plus `capacity` entrées. Un lookup est un hit si la query
/// présentée a cosine similarity > `hit_threshold` avec au moins une clé.
pub struct PredictiveCache {
    entries: VecDeque<CacheEntry>,
    pub capacity: usize,
    pub hit_threshold: f64,
    pub hits: u64,
    pub misses: u64,
}

impl PredictiveCache {
    pub fn new(capacity: usize, hit_threshold: f64) -> Self {
        Self {
            entries: VecDeque::with_capacity(capacity),
            capacity,
            hit_threshold,
            hits: 0,
            misses: 0,
        }
    }

    /// Insère une entrée (query, résultats). Évince la plus ancienne si plein.
    pub fn put(&mut self, query: Vec<f64>, results: Vec<(Vec<f64>, f64)>) {
        if self.entries.len() >= self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back(CacheEntry { query, results });
    }

    /// Lookup : retourne les résultats cachés si une clé matche assez bien.
    /// Met à jour les compteurs hits/misses.
    pub fn get(&mut self, query: &[f64]) -> Option<&Vec<(Vec<f64>, f64)>> {
        let mut best_sim = self.hit_threshold;
        let mut best_idx: Option<usize> = None;
        for (i, entry) in self.entries.iter().enumerate() {
            let sim = cosine(query, &entry.query);
            if sim > best_sim {
                best_sim = sim;
                best_idx = Some(i);
            }
        }
        match best_idx {
            Some(i) => {
                self.hits += 1;
                Some(&self.entries[i].results)
            }
            None => {
                self.misses += 1;
                None
            }
        }
    }

    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 { 0.0 } else { self.hits as f64 / total as f64 }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn len(&self) -> usize { self.entries.len() }
    pub fn is_empty(&self) -> bool { self.entries.is_empty() }
}

#[inline]
fn cosine(a: &[f64], b: &[f64]) -> f64 {
    let len = a.len().min(b.len());
    let mut dot = 0.0;
    let mut na = 0.0;
    let mut nb = 0.0;
    for i in 0..len {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    let denom = (na.sqrt() * nb.sqrt()).max(f64::EPSILON);
    dot / denom
}

// ==========================================================================
// Tests
// ==========================================================================
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_hit_for_similar_query() {
        let mut cache = PredictiveCache::new(4, 0.90);
        let q1 = vec![1.0, 0.0, 0.0, 0.0];
        let results = vec![(vec![0.5; 4], 0.8)];
        cache.put(q1.clone(), results.clone());

        // Query identique → hit certain
        let r = cache.get(&q1);
        assert!(r.is_some());
        assert_eq!(cache.hits, 1);

        // Query légèrement différente mais cosine > 0.90 → hit
        let q2 = vec![0.95, 0.05, 0.05, 0.05];
        let r2 = cache.get(&q2);
        assert!(r2.is_some(), "should hit for similar query (cos={})",
                cosine(&q1, &q2));
    }

    #[test]
    fn cache_miss_for_dissimilar_query() {
        let mut cache = PredictiveCache::new(4, 0.90);
        cache.put(vec![1.0, 0.0, 0.0, 0.0], vec![]);

        // Orthogonal → miss
        let r = cache.get(&[0.0, 1.0, 0.0, 0.0]);
        assert!(r.is_none());
        assert_eq!(cache.misses, 1);
    }

    #[test]
    fn cache_evicts_oldest_when_full() {
        let mut cache = PredictiveCache::new(2, 0.90);
        cache.put(vec![1.0, 0.0, 0.0, 0.0], vec![]);
        cache.put(vec![0.0, 1.0, 0.0, 0.0], vec![]);
        cache.put(vec![0.0, 0.0, 1.0, 0.0], vec![]);  // évince le premier

        assert_eq!(cache.len(), 2);
        // Le premier doit avoir été éjecté
        assert!(cache.get(&[1.0, 0.0, 0.0, 0.0]).is_none());
        // Le dernier doit être présent
        assert!(cache.get(&[0.0, 0.0, 1.0, 0.0]).is_some());
    }

    #[test]
    fn hit_rate_tracks_correctly() {
        let mut cache = PredictiveCache::new(4, 0.90);
        cache.put(vec![1.0, 0.0, 0.0, 0.0], vec![]);
        cache.get(&[1.0, 0.0, 0.0, 0.0]); // hit
        cache.get(&[0.0, 1.0, 0.0, 0.0]); // miss
        cache.get(&[1.0, 0.0, 0.0, 0.0]); // hit
        assert!((cache.hit_rate() - 2.0/3.0).abs() < 1e-6);
    }
}
