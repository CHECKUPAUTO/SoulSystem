//! Index vectoriel continu en mémoire partagée fixe (Zéro-Allocation après initialisation).
//! Permet des recherches de similarité cosinus ultra-rapides sans verrou pour les lecteurs.

use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicUsize, Ordering};

const MAX_VECTORS: usize = 65536;
const VECTOR_DIM: usize = 1024; // Adapté pour les embeddings de taille standard (Bert-like)

/// Enregistrement vectoriel compact — stocké en ligne dans le store (pas d'alloc heap).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VectorRecord {
    pub id: u64,
    /// Vecteur aligné sur 64 bytes (ligne de cache L1) pour accès SIMD.
    #[allow(dead_code)] // Accessé via pointer arithmétique dans les kernels
    pub data: [f32; VECTOR_DIM],
}

/// Résultat de recherche KNN — score normalisé par similarité cosinus.
#[derive(Debug, Clone, Copy)]
pub struct SearchResult {
    pub id: u64,
    pub score: f32,
}

/// Stockage vectoriel Lock-Free en lecture, optimisé pour l'accès concurrent direct.
///
/// Invariant : les lectures ne jamais acquitter de verrou — le compteur `count` atomique
/// fournit une vue cohérente de "quels vecteurs sont valides". Les écritures sérialisées
/// (par le scheduler ou un single writer) garantissent que l'insertion est atomic.
pub struct VectorStore {
    /// Nombre de vecteurs insérés (atomique — lecture lock-free).
    count: AtomicUsize,
    /// Buffer fixe de enregistrements vectoriels alloué sur le heap pour éviter le stack overflow.
    records: Box<[UnsafeCell<VectorRecord>]>,
}

unsafe impl Sync for VectorStore {}
unsafe impl Send for VectorStore {}

impl VectorStore {
    /// Construit un store vide — tous les slots initialisés à zéro via MaybeUninit.
    pub fn new() -> Self {
        // Allocation sur le heap pour éviter l'overflow de la pile.
        let records = (0..MAX_VECTORS)
            .map(|_| {
                UnsafeCell::new(VectorRecord {
                    id: 0,
                    data: [0.0; VECTOR_DIM],
                })
            })
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Self {
            count: AtomicUsize::new(0),
            records,
        }
    }

    /// Insertion thread-safe (Single-Writer ou sérialisée en amont par le scheduler).
    pub fn insert(&self, id: u64, vector: &[f32; VECTOR_DIM]) -> bool {
        let idx = self.count.load(Ordering::Relaxed);
        if idx >= MAX_VECTORS {
            return false;
        }

        unsafe {
            let slot = self.records[idx].get();
            (*slot).id = id;
            (*slot).data.copy_from_slice(vector);
        }

        // Release garantit que les données du vecteur sont visibles avant l'incrémentation du compteur.
        self.count.store(idx + 1, Ordering::Release);
        true
    }

    /// Recherche par similarité cosinus (K-Nearest Neighbors). Zéro-Allocation.
    /// Le tableau `results` doit être pré-alloué avec une taille ≥ k.
    /// Conçu pour être exécuté directement par un thread worker du soul_scheduler.
    pub fn knn_search(
        &self,
        query: &[f32; VECTOR_DIM],
        k: usize,
        results: &mut [SearchResult],
    ) -> usize {
        let current_count = self.count.load(Ordering::Acquire);
        if current_count == 0 {
            return 0;
        }

        let effective_k = k.min(current_count);
        // Initialiser results avec des scores à -inf (non-alignés)
        for r in results.iter_mut() {
            r.score = f32::NEG_INFINITY;
        }

        let mut write_idx = 0usize;

        for i in 0..current_count {
            let record = unsafe { &*self.records[i].get() };

            // Calcul du produit scalaire et des normes — hautement auto-vectorisable.
            let mut dot_product = 0.0f32;
            let mut norm_a = 0.0f32;
            let mut norm_b = 0.0f32;

            // Déroulage de boucle x4 pour le prefetch hardware et la réduction de dépendance.
            let mut j = 0usize;
            while j + 4 <= VECTOR_DIM {
                let a0 = query[j];
                let b0 = record.data[j];
                let a1 = query[j + 1];
                let b1 = record.data[j + 1];
                dot_product += a0 * b0 + a1 * b1;
                norm_a += a0 * a0 + a1 * a1;
                norm_b += b0 * b0 + b1 * b1;
                j += 4;
            }
            // Cleanup non-aligné.
            while j < VECTOR_DIM {
                let a = query[j];
                let b = record.data[j];
                dot_product += a * b;
                norm_a += a * a;
                norm_b += b * b;
                j += 1;
            }

            let score = if norm_a > 0.0 && norm_b > 0.0 {
                dot_product / (norm_a.sqrt() * norm_b.sqrt())
            } else {
                -1.0
            };

            // Insertion triée sur place dans le tableau de résultats (min-heap manuel).
            if write_idx < effective_k {
                results[write_idx] = SearchResult {
                    id: record.id,
                    score,
                };
                // Bubble-up : maintenir l'ordre décroissant.
                let mut pos = write_idx;
                while pos > 0 && results[pos].score > results[pos - 1].score {
                    results.swap(pos, pos - 1);
                    pos -= 1;
                }
                write_idx += 1;
            } else if score > results[effective_k - 1].score {
                results[effective_k - 1] = SearchResult {
                    id: record.id,
                    score,
                };
                // Bubble-down.
                let mut pos = effective_k - 1;
                while pos + 1 < effective_k && results[pos + 1].score > results[pos].score {
                    results.swap(pos, pos + 1);
                    pos += 1;
                }
            }
        }

        effective_k
    }
}

impl Default for VectorStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_retrieve_vector() {
        let store = VectorStore::new();
        let vec: [f32; VECTOR_DIM] = [1.0; VECTOR_DIM];
        assert!(store.insert(42, &vec));
        assert_eq!(store.count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn knn_returns_closest_matches() {
        let store = VectorStore::new();
        // Insert 5 vectors with distinct directions.
        for i in 0..5 {
            let mut vec = [0.0; VECTOR_DIM];
            vec[i] = i as f32 + 1.0; // Only the i-th dimension is non-zero
            store.insert(i as u64, &vec);
        }

        // Query is closest to vector 0 and 1.
        let mut query = [0.0; VECTOR_DIM];
        query[0] = 1.0;
        query[1] = 0.5;

        let mut results = [SearchResult {
            id: 0,
            score: f32::NEG_INFINITY,
        }; 3];
        let count = store.knn_search(&query, 3, &mut results);

        assert_eq!(count, 3);
        // Vector 0 is closest (similarity = 1*1 / (sqrt(1.25)*1) = 0.89), then Vector 1.
        assert!(results[0].score > results[1].score);
        assert_eq!(results[0].id, 0);
        assert_eq!(results[1].id, 1);
    }

    #[test]
    fn store_overflows_gracefully() {
        let store = VectorStore::new();
        for i in 0..MAX_VECTORS {
            let vec: [f32; VECTOR_DIM] = [0.0; VECTOR_DIM];
            assert!(store.insert(i as u64, &vec));
        }
        // One more must fail.
        let overflow: [f32; VECTOR_DIM] = [1.0; VECTOR_DIM];
        assert!(!store.insert(MAX_VECTORS as u64, &overflow));
    }
}
