//! Auto-reparation ontologique : detecte et repare les incoherences d'un etat
//! (NaN, infinis, hors-domaine) pour maintenir l'invariant [min, max].

/// Repare `state` en place : NaN/infini/< min -> min ; > max -> max.
/// Renvoie le nombre d'elements reparees.
pub fn heal(state: &mut [f32], min: f32, max: f32) -> usize {
    let mut repaired = 0;
    for v in state.iter_mut() {
        if !v.is_finite() || *v < min {
            *v = min;
            repaired += 1;
        } else if *v > max {
            *v = max;
            repaired += 1;
        }
    }
    repaired
}

/// Invariant satisfait : tout fini et dans [min, max].
pub fn is_consistent(state: &[f32], min: f32, max: f32) -> bool {
    state.iter().all(|v| v.is_finite() && *v >= min && *v <= max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repare_nan_inf_et_hors_borne() {
        let mut s = vec![0.5, f32::NAN, f32::INFINITY, -3.0, 2.0, 0.1];
        assert!(!is_consistent(&s, 0.0, 1.0));
        assert_eq!(heal(&mut s, 0.0, 1.0), 4);
        assert!(is_consistent(&s, 0.0, 1.0));
        assert_eq!(s, vec![0.5, 0.0, 0.0, 0.0, 1.0, 0.1]);
        println!("PREUVE self-healing : 4 reparations -> {:?}", s);
    }

    #[test]
    fn etat_sain_inchange() {
        let mut s = vec![0.0, 0.5, 1.0];
        let before = s.clone();
        assert_eq!(heal(&mut s, 0.0, 1.0), 0);
        assert_eq!(s, before);
        println!("PREUVE self-healing : etat sain -> 0 reparation");
    }
}
