use std::sync::atomic::{AtomicBool, Ordering};

const VECTOR_DIM: usize = 1024;

/// Contrôleur d'abliteration et d'injection de concepts comportementaux
pub struct NeuralSurgeon {
    pub is_active: AtomicBool,
    pub steering_vector: [f32; VECTOR_DIM],
    pub coefficient: f32,
}

impl NeuralSurgeon {
    pub fn new(coefficient: f32) -> Self {
        Self {
            is_active: AtomicBool::new(false),
            steering_vector: [0.0f32; VECTOR_DIM],
            coefficient,
        }
    }

    pub fn set_steering_target(&mut self, vector: &[f32; VECTOR_DIM]) {
        self.steering_vector.copy_from_slice(vector);
        self.is_active.store(true, Ordering::Release);
    }

    /// Applique une déviation orthogonale ou une abliteration sur un tenseur d'activation intermédiaire.
    /// Équation fondamentale de guidage : $x \leftarrow x + \alpha \cdot v_{steer}$
    #[inline(always)]
    pub fn steer_activations(&self, activations: &mut [f32]) {
        if !self.is_active.load(Ordering::Acquire) { return; }

        // Traitement par bloc de taille VECTOR_DIM (Auto-vectorisation SIMD forcée)
        for (idx, chunk) in activations.chunks_mut(VECTOR_DIM).enumerate() {
            let offset = idx * VECTOR_DIM;
            for (i, val) in chunk.iter_mut().enumerate() {
                *val += self.coefficient * self.steering_vector[offset + i];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    // Couche lineaire reelle : out[o] = sum_i W[o,i]*h[i] (W ligne-major, out_dim x dim).
    fn linear(w: &[f32], h: &[f32], out_dim: usize) -> Vec<f32> {
        let dim = h.len();
        let mut out = vec![0.0f32; out_dim];
        for o in 0..out_dim {
            let mut s = 0.0f32;
            for i in 0..dim {
                s += w[o * dim + i] * h[i];
            }
            out[o] = s;
        }
        out
    }

    #[test]
    fn steering_modifie_la_sortie_dun_vrai_forward_pass() {
        let dim = VECTOR_DIM;
        let h_clean: Vec<f32> = (0..dim).map(|i| ((i % 7) as f32) * 0.1 - 0.3).collect();
        let out_dim = 2;
        let w: Vec<f32> = (0..out_dim * dim).map(|i| ((i % 5) as f32 - 2.0) * 0.05).collect();
        let clean_out = linear(&w, &h_clean, out_dim);

        let mut surgeon = NeuralSurgeon::new(0.5);
        let mut v = [0.0f32; VECTOR_DIM];
        for i in 0..dim {
            v[i] = if i % 2 == 0 { 1.0 } else { -1.0 };
        }
        surgeon.set_steering_target(&v);
        assert!(surgeon.is_active.load(Ordering::Acquire));

        let mut h = h_clean.clone();
        surgeon.steer_activations(&mut h); // h <- h + 0.5 * v
        for i in 0..dim {
            assert!((h[i] - (h_clean[i] + 0.5 * v[i])).abs() < 1e-6);
        }
        let steered_out = linear(&w, &h, out_dim);

        let mut changed = false;
        for o in 0..out_dim {
            let mut expected = 0.0f32;
            for i in 0..dim {
                expected += w[o * dim + i] * (0.5 * v[i]);
            }
            let delta = steered_out[o] - clean_out[o];
            assert!((delta - expected).abs() < 1e-2, "delta[{}]={} attendu {}", o, delta, expected);
            if delta.abs() > 1e-6 {
                changed = true;
            }
        }
        assert!(changed, "le steering doit modifier la sortie");
        println!("PREUVE steering : clean={:?} -> steered={:?} (delta = W*alpha*v, non nul)", clean_out, steered_out);
    }

    #[test]
    fn steering_inactif_est_un_noop() {
        let surgeon = NeuralSurgeon::new(0.9); // jamais arme
        let mut h: Vec<f32> = vec![0.5; VECTOR_DIM];
        let before = h.clone();
        surgeon.steer_activations(&mut h);
        assert_eq!(h, before, "inactif -> aucune modification");
        println!("PREUVE no-op si inactif : activation inchangee");
    }
}
