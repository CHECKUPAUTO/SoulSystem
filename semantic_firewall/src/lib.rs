//! Pare-feu semantique reel : bloque un vecteur (embedding / activation) dont la
//! similarite cosinus avec l'un des concepts interdits atteint le seuil.
//! Les ancres (embeddings des concepts a bannir) sont INJECTEES par le
//! proprietaire de l'embedder ; ce module est le moteur de decision, deterministe
//! et sans reseau (fidele au zero-syscall).

use scirust::autodiff::reverse::Tensor;

pub struct FirewallGuard {
    pub threshold: f32,
    /// Embeddings L2-normalises des concepts interdits.
    anchors: Vec<Vec<f32>>,
}

impl FirewallGuard {
    /// Seuil par defaut 0.85, sans ancre. Tant qu'aucun concept interdit n'est
    /// enregistre, tout passe (fail-open documente, a peupler par l'appelant).
    pub fn new() -> Self {
        Self { threshold: 0.85, anchors: Vec::new() }
    }

    pub fn with_threshold(threshold: f32) -> Self {
        Self { threshold, anchors: Vec::new() }
    }

    /// Enregistre un concept interdit a partir de son embedding (normalise L2 a
    /// l'enregistrement). Renvoie false si le vecteur est nul/non-normalisable.
    pub fn register_forbidden(&mut self, anchor: &Tensor) -> bool {
        match normalize(&anchor.data) {
            Some(v) => { self.anchors.push(v); true }
            None => false,
        }
    }

    #[inline]
    pub fn forbidden_count(&self) -> usize {
        self.anchors.len()
    }

    /// Similarite cosinus MAXIMALE entre `vector` et les concepts interdits.
    /// 0.0 si aucune ancre comparable ou vecteur nul.
    pub fn max_similarity(&self, vector: &Tensor) -> f32 {
        let v = match normalize(&vector.data) {
            Some(v) => v,
            None => return 0.0,
        };
        let mut best = f32::NEG_INFINITY;
        for a in &self.anchors {
            if a.len() != v.len() {
                continue; // dimensions incompatibles -> ignore
            }
            let dot: f32 = a.iter().zip(&v).map(|(x, y)| x * y).sum();
            if dot > best {
                best = dot;
            }
        }
        if best.is_finite() { best } else { 0.0 }
    }

    /// Verdict de surete : `true` = autorise, `false` = bloque.
    /// Bloque si la similarite cosinus avec un concept interdit atteint le seuil.
    pub fn check_safety(&self, vector: &Tensor) -> bool {
        self.max_similarity(vector) < self.threshold
    }
}

impl Default for FirewallGuard {
    fn default() -> Self {
        Self::new()
    }
}

/// Normalise L2 un vecteur. None si vide / norme nulle / non-finie.
fn normalize(data: &[f32]) -> Option<Vec<f32>> {
    if data.is_empty() {
        return None;
    }
    let norm_sq: f32 = data.iter().map(|x| x * x).sum();
    if norm_sq <= 0.0 || !norm_sq.is_finite() {
        return None;
    }
    let inv = 1.0 / norm_sq.sqrt();
    Some(data.iter().map(|x| x * inv).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use scirust::autodiff::reverse::Tensor;

    fn vec_tensor(v: Vec<f32>) -> Tensor {
        let n = v.len();
        Tensor::from_vec(v, 1, n)
    }

    #[test]
    fn bloque_vecteur_proche_dun_concept_interdit() {
        let mut fw = FirewallGuard::new(); // seuil 0.85
        assert!(fw.register_forbidden(&vec_tensor(vec![1.0, 0.0, 0.0, 0.0])));
        assert_eq!(fw.forbidden_count(), 1);

        let proche = vec_tensor(vec![0.9, 0.1, 0.0, 0.0]); // quasi colineaire
        let sim = fw.max_similarity(&proche);
        assert!(sim > 0.85, "similarite attendue > seuil, obtenu {}", sim);
        assert!(!fw.check_safety(&proche), "doit bloquer un vecteur proche du concept interdit");
        println!("PREUVE block : cos={:.3} >= 0.85 -> check_safety=false", sim);
    }

    #[test]
    fn laisse_passer_vecteur_eloigne() {
        let mut fw = FirewallGuard::new();
        fw.register_forbidden(&vec_tensor(vec![1.0, 0.0, 0.0, 0.0]));

        let ortho = vec_tensor(vec![0.0, 1.0, 0.0, 0.0]); // cos 0
        assert!(fw.check_safety(&ortho), "orthogonal doit passer");
        println!("PREUVE allow ortho : cos={:.3} < 0.85 -> true", fw.max_similarity(&ortho));

        let sous_seuil = vec_tensor(vec![1.0, 1.0, 0.0, 0.0]); // cos 0.707
        let sim = fw.max_similarity(&sous_seuil);
        assert!(sim < 0.85, "cos {} doit etre < seuil", sim);
        assert!(fw.check_safety(&sous_seuil));
        println!("PREUVE allow sous-seuil : cos={:.3} < 0.85 -> true", sim);
    }

    #[test]
    fn sans_ancre_tout_passe_et_vecteur_nul_neutre() {
        let fw = FirewallGuard::new();
        assert!(fw.check_safety(&vec_tensor(vec![1.0, 2.0, 3.0, 4.0]))); // fail-open

        let mut fw2 = FirewallGuard::new();
        assert!(!fw2.register_forbidden(&vec_tensor(vec![0.0, 0.0, 0.0, 0.0])), "vecteur nul non ancrable");
        assert_eq!(fw2.forbidden_count(), 0);
        assert!(fw2.check_safety(&vec_tensor(vec![0.0, 0.0, 0.0, 0.0])));
        println!("PREUVE garde-fous : sans ancre tout passe, vecteur nul neutre");
    }

    #[test]
    fn plusieurs_concepts_le_max_decide() {
        let mut fw = FirewallGuard::with_threshold(0.9);
        fw.register_forbidden(&vec_tensor(vec![1.0, 0.0, 0.0, 0.0]));
        fw.register_forbidden(&vec_tensor(vec![0.0, 0.0, 1.0, 0.0]));
        let v = vec_tensor(vec![0.05, 0.0, 0.99, 0.0]); // proche du 2e concept
        let sim = fw.max_similarity(&v);
        assert!(!fw.check_safety(&v), "doit bloquer (proche concept 2)");
        println!("PREUVE multi-ancres : max cos={:.3} >= 0.9 -> bloque", sim);
    }
}
