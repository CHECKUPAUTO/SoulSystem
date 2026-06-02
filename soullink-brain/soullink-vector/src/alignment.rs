// =============================================================================
// PATCH 1 : crates/memory/src/alignment.rs
// Alignement SIMD garanti à l'upsert + registre de dimension
// =============================================================================
//
// PROBLÈME RÉEL (non mentionné dans la review) :
//   Le graphe peut stocker en même temps des vecteurs de dim 384 (MiniLM)
//   et de dim 512 (BGE-small) si deux modèles sont actifs. La recherche
//   cosine sur des dimensions différentes retourne 0.0 silencieusement.
//
// SOLUTION :
//   DimensionRegistry : le premier vecteur inséré fixe la dimension.
//   Tout vecteur de dimension différente est refusé avec une erreur claire.
//   Padding à l'upsert (une seule allocation, pas à chaque recherche).
// =============================================================================

use crate::error::{Result, VectorError};
use std::sync::atomic::{AtomicUsize, Ordering};

// ─────────────────────────────────────────────────────────────────────────────
// DIMENSION REGISTRY
// ─────────────────────────────────────────────────────────────────────────────

/// Registre global de la dimension vectorielle attendue.
/// Initialisé au premier upsert, immuable ensuite.
pub struct DimensionRegistry {
    /// 0 = non initialisé
    dim: AtomicUsize,
    /// Dimension padded (multiple de 8), dérivée de `dim`
    dim_padded: AtomicUsize,
}

impl Default for DimensionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl DimensionRegistry {
    pub const fn new() -> Self {
        Self {
            dim: AtomicUsize::new(0),
            dim_padded: AtomicUsize::new(0),
        }
    }

    /// Enregistre la dimension au premier appel.
    /// Les appels suivants vérifient la cohérence.
    pub fn register_or_check(&self, incoming_dim: usize) -> Result<usize> {
        let current = self.dim.load(Ordering::Acquire);

        if current == 0 {
            // Première insertion : fixer la dimension
            let padded = pad_dim(incoming_dim);
            // Compare-and-swap : thread-safe
            match self
                .dim
                .compare_exchange(0, incoming_dim, Ordering::Release, Ordering::Acquire)
            {
                Ok(_) => {
                    self.dim_padded.store(padded, Ordering::Release);
                    tracing::info!(
                        "DimensionRegistry: locked at dim={} padded={}",
                        incoming_dim,
                        padded
                    );
                    Ok(padded)
                }
                Err(actual) => {
                    // Une autre thread a gagné la course — vérifier cohérence
                    self.check(actual, incoming_dim)
                }
            }
        } else {
            self.check(current, incoming_dim)
        }
    }

    fn check(&self, expected: usize, got: usize) -> Result<usize> {
        if got != expected && got != self.dim_padded.load(Ordering::Acquire) {
            return Err(VectorError::GraphPoisoned(format!(
                "Vector dimension mismatch: registry={}, got={}. \
                 Change model? Clear the vector store first.",
                expected, got
            )));
        }
        Ok(self.dim_padded.load(Ordering::Acquire))
    }

    pub fn padded_dim(&self) -> Option<usize> {
        let d = self.dim_padded.load(Ordering::Acquire);
        if d == 0 {
            None
        } else {
            Some(d)
        }
    }

    pub fn raw_dim(&self) -> Option<usize> {
        let d = self.dim.load(Ordering::Acquire);
        if d == 0 {
            None
        } else {
            Some(d)
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// FONCTIONS D'ALIGNEMENT
// ─────────────────────────────────────────────────────────────────────────────

/// Calcule la dimension padded (multiple de 8).
/// Identique au bit-trick de la review : `(n + 7) & !7`
#[inline(always)]
pub fn pad_dim(n: usize) -> usize {
    (n + 7) & !7
}

/// Aligne un vecteur à la dimension cible en ajoutant des zéros.
/// Appelé UNE FOIS à l'upsert → plus d'allocation à la recherche.
pub fn align_vector(mut v: Vec<f32>, target_dim: usize) -> Vec<f32> {
    debug_assert_eq!(target_dim % 8, 0, "target_dim must be multiple of 8");
    match v.len().cmp(&target_dim) {
        std::cmp::Ordering::Equal => v,
        std::cmp::Ordering::Less => {
            v.resize(target_dim, 0.0);
            v
        }
        std::cmp::Ordering::Greater => {
            // Troncature : log un warning, ne pas silencieusement corrompre
            tracing::warn!(
                "align_vector: truncating {} → {} dims (data loss!)",
                v.len(),
                target_dim
            );
            v.truncate(target_dim);
            v
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// INTÉGRATION DANS MemoryGraph::upsert (diff partiel)
// ─────────────────────────────────────────────────────────────────────────────

// Dans MemoryGraph, ajouter :
//
//   pub dim_registry: DimensionRegistry,
//
// Et modifier upsert() :
//
//   pub fn upsert(&self, concept: Concept, embedding: Option<Vec<f32>>) -> Result<NodeIndex> {
//       let embedding = match embedding {
//           None => None,
//           Some(raw) => {
//               // ① Valider et récupérer la dim padded
//               let padded_dim = self.dim_registry.register_or_check(raw.len())?;
//               // ② Aligner une seule fois ici
//               Some(align_vector(raw, padded_dim))
//           }
//       };
//       // ... reste inchangé
//   }

// ─────────────────────────────────────────────────────────────────────────────
// TESTS
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pad_dim_correct() {
        assert_eq!(pad_dim(384), 384); // déjà multiple de 8
        assert_eq!(pad_dim(512), 512); // idem
        assert_eq!(pad_dim(383), 384); // arrondi vers le haut
        assert_eq!(pad_dim(1), 8);
        assert_eq!(pad_dim(9), 16);
        assert_eq!(pad_dim(0), 0); // cas limite
    }

    #[test]
    fn align_vector_pads_with_zeros() {
        let v = vec![1.0f32, 2.0, 3.0];
        let aligned = align_vector(v, 8);
        assert_eq!(aligned.len(), 8);
        assert_eq!(&aligned[3..], &[0.0f32; 5]);
    }

    #[test]
    fn align_vector_noop_when_aligned() {
        let v: Vec<f32> = (0..384).map(|i| i as f32).collect();
        let v2 = v.clone();
        let aligned = align_vector(v, 384);
        assert_eq!(aligned, v2); // aucune allocation supplémentaire
    }

    #[test]
    fn dimension_registry_locks_on_first_insert() {
        let reg = DimensionRegistry::new();
        assert!(reg.padded_dim().is_none());

        let dim = reg.register_or_check(384).unwrap();
        assert_eq!(dim, 384); // 384 déjà multiple de 8
        assert_eq!(reg.raw_dim(), Some(384));

        // Même dimension → OK
        assert!(reg.register_or_check(384).is_ok());

        // Dimension différente → erreur explicite
        let err = reg.register_or_check(512).unwrap_err();
        assert!(err.to_string().contains("mismatch"));
    }

    #[test]
    fn dimension_registry_pads_correctly() {
        let reg = DimensionRegistry::new();
        // 383 n'est pas multiple de 8 → padded = 384
        let padded = reg.register_or_check(383).unwrap();
        assert_eq!(padded, 384);
    }
}
