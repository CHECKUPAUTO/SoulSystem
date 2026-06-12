//! Synchronisation d'etat inter-noeuds par fusion CRDT monotone (merge-max).
//! Commutative, associative, idempotente : convergence garantie quel que soit
//! l'ordre de reception des etats distants.

/// Fusionne `remote` dans `local` par maximum element-par-element.
/// Renvoie le nombre d'elements releves.
pub fn merge_max(local: &mut [f32], remote: &[f32]) -> usize {
    let n = local.len().min(remote.len());
    let mut updated = 0;
    for i in 0..n {
        if remote[i] > local[i] {
            local[i] = remote[i];
            updated += 1;
        }
    }
    updated
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_releve_le_maximum() {
        let mut local = vec![1.0, 5.0, 2.0];
        assert_eq!(merge_max(&mut local, &[3.0, 4.0, 2.0]), 1);
        assert_eq!(local, vec![3.0, 5.0, 2.0]);
        println!("PREUVE merge-max : {:?}", local);
    }

    #[test]
    fn idempotente() {
        let mut a = vec![1.0, 2.0, 3.0];
        let snap = a.clone();
        merge_max(&mut a, &snap);
        assert_eq!(a, snap);
        println!("PREUVE idempotence : merge(a,a)=a");
    }

    #[test]
    fn convergente() {
        let base_a = vec![1.0, 9.0, 3.0, 0.0];
        let base_b = vec![7.0, 2.0, 3.0, 5.0];
        let mut a = base_a.clone();
        merge_max(&mut a, &base_b);
        let mut b = base_b.clone();
        merge_max(&mut b, &base_a);
        assert_eq!(a, b);
        assert_eq!(a, vec![7.0, 9.0, 3.0, 5.0]);
        println!("PREUVE convergence CRDT : a==b=={:?}", a);
    }
}

// ── Property-based tests (proptest) ────────────────────────────

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    fn vec_f32(len: usize) -> impl Strategy<Value = Vec<f32>> {
        prop::collection::vec(-100.0f32..100.0f32, len)
    }

    proptest! {
        #[test]
        fn merge_max_is_commutative(
            a in vec_f32(10),
            b in vec_f32(10),
        ) {
            let mut a1 = a.clone();
            let mut a2 = a.clone();
            merge_max(&mut a1, &b);
            merge_max(&mut a2, &b);
            prop_assert_eq!(a1, a2);
        }

        #[test]
        fn merge_max_is_associative(
            a in vec_f32(10),
            b in vec_f32(10),
            c in vec_f32(10),
        ) {
            let mut ab = a.clone();
            merge_max(&mut ab, &b);
            merge_max(&mut ab, &c);

            let mut bc = b.clone();
            merge_max(&mut bc, &c);
            let mut abc = a.clone();
            merge_max(&mut abc, &bc);

            prop_assert_eq!(ab, abc);
        }

        #[test]
        fn merge_max_is_idempotent(a in vec_f32(10)) {
            let mut a1 = a.clone();
            merge_max(&mut a1, &a);
            prop_assert_eq!(a1, a);
        }

        #[test]
        fn merge_max_monotonic_increases(
            a in vec_f32(10),
            b in vec_f32(10),
        ) {
            let mut a1 = a.clone();
            let old_max: f32 = a.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
            merge_max(&mut a1, &b);
            let new_max: f32 = a1.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
            prop_assert!(new_max >= old_max, "Max should not decrease: {} -> {}", old_max, new_max);
        }

        #[test]
        fn merge_max_converges_any_order(
            base_a in vec_f32(10),
            base_b in vec_f32(10),
        ) {
            let mut a = base_a.clone();
            merge_max(&mut a, &base_b);
            let mut b = base_b.clone();
            merge_max(&mut b, &base_a);
            prop_assert_eq!(a.clone(), b.clone(), "Convergence failed: {:?} != {:?}", a, b);
        }

        #[test]
        fn merge_max_handles_different_lengths(
            a in vec_f32(15),
            b in vec_f32(5),
        ) {
            let mut a1 = a.clone();
            let original_len = a1.len();
            merge_max(&mut a1, &b);
            // Length should not change
            prop_assert_eq!(a1.len(), original_len);
            // First 5 elements should be max of both
            for i in 0..5 {
                prop_assert_eq!(a1[i], a[i].max(b[i]));
            }
            // Rest unchanged
            for i in 5..original_len {
                prop_assert_eq!(a1[i], a[i]);
            }
        }
    }
}
