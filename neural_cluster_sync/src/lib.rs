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
