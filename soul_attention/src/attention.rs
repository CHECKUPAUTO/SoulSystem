//! Attention scaled-dot-product sur les positions actives du KV-cache.
//!
//! Consommateur naturel de `KvCache` : score la query contre chaque cle active
//! (sinks + fenetre), softmax numeriquement stable (max-shift), sortie = somme
//! ponderee des valeurs. Brique d'attention single-head ; pas un modele complet.

use crate::cache::KvCache;

/// Variante zero-allocation : ecrit la sortie dans `out` (taille = dim) en
/// utilisant `scores` comme scratch (longueur >= cache.active_len()).
pub fn attend_into(cache: &KvCache, query: &[f32], scores: &mut [f32], out: &mut [f32]) {
    let dim = cache.dim();
    assert_eq!(query.len(), dim, "query dim != cache dim");
    assert_eq!(out.len(), dim, "out len != cache dim");
    let active = cache.active_len();
    assert!(scores.len() >= active, "scores scratch trop petit");

    out.iter_mut().for_each(|o| *o = 0.0);
    if active == 0 {
        return; // cache vide : rien a quoi attendre
    }
    let scale = 1.0 / (dim as f32).sqrt();

    // 1) scores bruts s_i = <q, K_i> / sqrt(dim) + max (stabilite softmax)
    let mut max_score = f32::NEG_INFINITY;
    for ((_, k, _), s) in cache.active().zip(scores.iter_mut()) {
        let dot: f32 = query.iter().zip(k).map(|(a, b)| a * b).sum();
        *s = dot * scale;
        if *s > max_score {
            max_score = *s;
        }
    }
    // 2) softmax stable
    let mut denom = 0.0f32;
    for s in scores[..active].iter_mut() {
        *s = (*s - max_score).exp();
        denom += *s;
    }
    // 3) sortie = somme ponderee des V actives
    for ((_, _, v), &sw) in cache.active().zip(scores[..active].iter()) {
        let w = sw / denom;
        for (o, &vi) in out.iter_mut().zip(v) {
            *o += w * vi;
        }
    }
}

/// Convenance (alloue scratch + sortie) ; pour le chemin chaud, voir `attend_into`.
pub fn attend(cache: &KvCache, query: &[f32]) -> Vec<f32> {
    let active = cache.active_len();
    let mut scores = vec![0.0f32; active.max(1)];
    let mut out = vec![0.0f32; cache.dim()];
    attend_into(cache, query, &mut scores, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::KvCache;

    fn approx(a: &[f32], b: &[f32], tol: f32) -> bool {
        a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() < tol)
    }

    #[test]
    fn single_key_returns_its_value() {
        let mut c = KvCache::new(4, 1, 4);
        c.push(&[1.0, 0.0, 0.0, 0.0], &[9.0, 8.0, 7.0, 6.0]);
        let out = attend(&c, &[1.0, 0.0, 0.0, 0.0]);
        assert!(approx(&out, &[9.0, 8.0, 7.0, 6.0], 1e-5), "out={out:?}");
    }

    #[test]
    fn equal_scores_average_values() {
        let mut c = KvCache::new(4, 1, 4);
        c.push(&[1.0, 0.0, 0.0, 0.0], &[2.0, 2.0, 2.0, 2.0]);
        c.push(&[0.0, 1.0, 0.0, 0.0], &[4.0, 4.0, 4.0, 4.0]);
        let out = attend(&c, &[1.0, 1.0, 0.0, 0.0]); // dots egaux -> 0.5/0.5
        assert!(approx(&out, &[3.0, 3.0, 3.0, 3.0], 1e-5), "out={out:?}");
    }

    #[test]
    fn dominant_key_wins() {
        let mut c = KvCache::new(4, 1, 4);
        c.push(&[1.0, 0.0, 0.0, 0.0], &[1.0, 1.0, 1.0, 1.0]);
        c.push(&[10.0, 0.0, 0.0, 0.0], &[5.0, 5.0, 5.0, 5.0]);
        let out = attend(&c, &[1.0, 0.0, 0.0, 0.0]);
        assert!(approx(&out, &[5.0, 5.0, 5.0, 5.0], 0.1), "out={out:?}");
    }

    #[test]
    fn empty_cache_returns_zeros() {
        let c = KvCache::new(4, 1, 2);
        assert_eq!(attend(&c, &[1.0, 0.0, 0.0, 0.0]), vec![0.0; 4]);
    }

    #[test]
    fn attends_over_sink_after_window_wrap() {
        let mut c = KvCache::new(2, 1, 2); // sink 0, fenetre 2
        c.push(&[1.0, 0.0], &[100.0, 100.0]); // sink (pos 0), cle [1,0]
        for p in 1..6 {
            c.push(&[0.0, 1.0], &[p as f32, p as f32]); // fenetre, cles [0,1]
        }
        // actives = sink{0} + fenetre{4,5} ; query fortement alignee sur le sink
        let out = attend(&c, &[5.0, 0.0]);
        assert!(
            out[0] > 90.0 && out[1] > 90.0,
            "le sink doit dominer apres wrap, out={out:?}"
        );
    }

    #[test]
    fn attend_into_matches_attend() {
        let mut c = KvCache::new(3, 1, 3);
        c.push(&[1.0, 0.0, 0.0], &[1.0, 2.0, 3.0]);
        c.push(&[0.0, 1.0, 0.0], &[4.0, 5.0, 6.0]);
        let conv = attend(&c, &[0.5, 0.5, 0.0]);
        let mut scores = [0.0f32; 8];
        let mut out = [0.0f32; 3];
        attend_into(&c, &[0.5, 0.5, 0.0], &mut scores, &mut out);
        assert!(approx(&conv, &out, 1e-6), "conv={conv:?} out={out:?}");
    }

    #[test]
    #[should_panic(expected = "query dim")]
    fn wrong_query_dim_panics() {
        let mut c = KvCache::new(4, 1, 2);
        c.push(&[1.0, 0.0, 0.0, 0.0], &[1.0, 1.0, 1.0, 1.0]);
        attend(&c, &[1.0, 0.0]);
    }
}
