//! KV-cache borne a fenetre glissante + attention sinks (StreamingLLM).
//!
//! Un cache PAR COUCHE : le modele en detient `n_layers`, avances en lockstep.
//! L'autoregression est sequentielle par sequence -> ecrivain unique : `push`
//! prend `&mut self`, la lecture `&self`. Aucune aliasing, donc ZERO `unsafe`
//! (contrairement au &self + *mut non sound) et zero allocation sur le chemin
//! chaud (buffers pre-alloues, ecriture par `copy_from_slice`).
//!
//! Eviction = sinks + fenetre : les `n_sink` premieres positions ne sont JAMAIS
//! evincees (elles ancrent l'attention, cf. "attention sinks"), et les
//! `n_window` positions les plus recentes vivent dans un anneau. Sans les sinks,
//! une fenetre FIFO naive jette les premiers tokens et degrade fortement la
//! qualite au-dela de la fenetre.

/// Cache K/V d'une couche : sinks + fenetre glissante, taille bornee fixe.
pub struct KvCache {
    dim: usize,
    n_sink: usize,
    n_window: usize,
    keys: Vec<f32>,   // (n_sink + n_window) * dim
    values: Vec<f32>, // idem
    len: usize,       // nombre total de tokens pousses (positions logiques vues)
}

impl KvCache {
    /// `dim` = dimension d'un vecteur K (== V) pour cette couche (souvent
    /// n_heads * head_dim). `n_sink` ancres, `n_window` glissants (> 0).
    pub fn new(dim: usize, n_sink: usize, n_window: usize) -> Self {
        assert!(dim > 0, "dim doit etre > 0");
        assert!(n_window > 0, "n_window doit etre > 0");
        let capacity = n_sink + n_window;
        Self {
            dim,
            n_sink,
            n_window,
            keys: vec![0.0; capacity * dim],
            values: vec![0.0; capacity * dim],
            len: 0,
        }
    }

    #[inline]
    pub fn dim(&self) -> usize {
        self.dim
    }
    #[inline]
    pub fn n_sink(&self) -> usize {
        self.n_sink
    }
    #[inline]
    pub fn n_window(&self) -> usize {
        self.n_window
    }
    /// Capacite physique (sinks + fenetre) = nb max de positions actives.
    #[inline]
    pub fn capacity(&self) -> usize {
        self.n_sink + self.n_window
    }
    /// Nombre total de tokens pousses depuis la creation/reset.
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Slot physique d'une position logique (usage interne).
    #[inline]
    fn slot_of(&self, pos: usize) -> usize {
        if pos < self.n_sink {
            pos
        } else {
            self.n_sink + (pos - self.n_sink) % self.n_window
        }
    }

    /// Position logique du plus ancien token de fenetre encore vivant.
    #[inline]
    fn window_start(&self) -> usize {
        self.len.saturating_sub(self.n_window).max(self.n_sink)
    }

    /// Ecrit le K/V du nouveau token et renvoie sa position logique. Zero alloc :
    /// ecriture dans un slot pre-alloue. Fenetre pleine -> ecrase la position la
    /// plus ancienne de l'anneau (les sinks restent).
    pub fn push(&mut self, k: &[f32], v: &[f32]) -> usize {
        assert_eq!(k.len(), self.dim, "longueur de K != dim");
        assert_eq!(v.len(), self.dim, "longueur de V != dim");
        let pos = self.len;
        let off = self.slot_of(pos) * self.dim;
        self.keys[off..off + self.dim].copy_from_slice(k);
        self.values[off..off + self.dim].copy_from_slice(v);
        self.len += 1;
        pos
    }

    /// Nombre de positions actives (sinks effectifs + fenetre effective).
    pub fn active_len(&self) -> usize {
        let sinks = self.n_sink.min(self.len);
        let win = self.len.saturating_sub(self.n_sink).min(self.n_window);
        sinks + win
    }

    /// Itere les positions actives dans l'ordre d'attention : sinks (0..) puis
    /// fenetre du plus ancien au plus recent. Item = (pos_logique, &K, &V).
    /// Emprunte le cache -> zero allocation.
    pub fn active(&self) -> impl Iterator<Item = (usize, &[f32], &[f32])> + '_ {
        let sinks = self.n_sink.min(self.len);
        let win_start = self.window_start();
        (0..sinks).chain(win_start..self.len).map(move |pos| {
            let off = self.slot_of(pos) * self.dim;
            (
                pos,
                &self.keys[off..off + self.dim],
                &self.values[off..off + self.dim],
            )
        })
    }

    /// K/V d'une position, ou None si evincee de la fenetre ou jamais ecrite.
    pub fn get(&self, pos: usize) -> Option<(&[f32], &[f32])> {
        if pos >= self.len {
            return None;
        }
        let is_sink = pos < self.n_sink;
        let in_window = pos >= self.window_start();
        if !is_sink && !in_window {
            return None; // evincee
        }
        let off = self.slot_of(pos) * self.dim;
        Some((
            &self.keys[off..off + self.dim],
            &self.values[off..off + self.dim],
        ))
    }

    /// Reinitialise pour une nouvelle sequence SANS reallouer (reutilise les
    /// buffers). Les anciennes donnees deviennent inaccessibles (len = 0).
    pub fn reset(&mut self) {
        self.len = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vec_for(pos: usize, dim: usize, tag: f32) -> Vec<f32> {
        (0..dim)
            .map(|i| tag + pos as f32 * 100.0 + i as f32)
            .collect()
    }

    #[test]
    fn push_within_sinks() {
        let mut c = KvCache::new(4, 2, 3);
        for p in 0..2 {
            assert_eq!(c.push(&vec_for(p, 4, 1.0), &vec_for(p, 4, 2.0)), p);
        }
        assert_eq!(c.len(), 2);
        assert_eq!(c.active_len(), 2);
        let got: Vec<usize> = c.active().map(|(p, _, _)| p).collect();
        assert_eq!(got, vec![0, 1]);
    }

    #[test]
    fn roundtrip_data() {
        let mut c = KvCache::new(3, 1, 4);
        for p in 0..3 {
            c.push(&vec_for(p, 3, 1.0), &vec_for(p, 3, 2.0));
        }
        for (p, k, v) in c.active() {
            assert_eq!(k, vec_for(p, 3, 1.0).as_slice());
            assert_eq!(v, vec_for(p, 3, 2.0).as_slice());
        }
    }

    #[test]
    fn window_slides_keeping_sinks() {
        let dim = 2;
        let mut c = KvCache::new(dim, 2, 3); // capacite active = 5
        for p in 0..10 {
            c.push(&vec_for(p, dim, 1.0), &vec_for(p, dim, 2.0));
        }
        assert_eq!(c.len(), 10);
        assert_eq!(c.active_len(), 5);
        let positions: Vec<usize> = c.active().map(|(p, _, _)| p).collect();
        assert_eq!(positions, vec![0, 1, 7, 8, 9]); // sinks 0,1 + 3 plus recents
        for (p, k, v) in c.active() {
            assert_eq!(k, vec_for(p, dim, 1.0).as_slice(), "K pos {p}");
            assert_eq!(v, vec_for(p, dim, 2.0).as_slice(), "V pos {p}");
        }
    }

    #[test]
    fn evicted_positions_return_none() {
        let mut c = KvCache::new(2, 1, 2); // sink 0, fenetre {., .} ; capacite 3
        for p in 0..6 {
            c.push(&vec_for(p, 2, 1.0), &vec_for(p, 2, 2.0));
        }
        assert!(c.get(0).is_some(), "sink vivant");
        assert!(c.get(5).is_some());
        assert!(c.get(4).is_some());
        assert!(c.get(3).is_none(), "evincee");
        assert!(c.get(2).is_none(), "evincee");
        assert!(c.get(6).is_none(), "jamais ecrite");
        let (k, _) = c.get(0).expect("sink");
        assert_eq!(
            k,
            vec_for(0, 2, 1.0).as_slice(),
            "sink non corrompu apres wrap"
        );
    }

    #[test]
    fn no_sinks_pure_window() {
        let mut c = KvCache::new(2, 0, 3);
        for p in 0..7 {
            c.push(&vec_for(p, 2, 1.0), &vec_for(p, 2, 2.0));
        }
        assert_eq!(c.active_len(), 3);
        let positions: Vec<usize> = c.active().map(|(p, _, _)| p).collect();
        assert_eq!(positions, vec![4, 5, 6]);
    }

    #[test]
    fn reset_reuses_buffers() {
        let mut c = KvCache::new(2, 1, 2);
        let ptr_before = c.keys.as_ptr();
        for p in 0..5 {
            c.push(&[p as f32; 2], &[p as f32; 2]);
        }
        c.reset();
        assert_eq!(c.len(), 0);
        assert_eq!(c.active_len(), 0);
        assert!(c.active().next().is_none());
        c.push(&[42.0; 2], &[42.0; 2]);
        assert_eq!(c.len(), 1);
        assert_eq!(
            c.keys.as_ptr(),
            ptr_before,
            "buffer reutilise, pas realloue"
        );
    }

    #[test]
    #[should_panic(expected = "longueur de K")]
    fn push_wrong_dim_panics() {
        let mut c = KvCache::new(4, 1, 2);
        c.push(&[1.0, 2.0], &[1.0, 2.0, 3.0, 4.0]);
    }
}
