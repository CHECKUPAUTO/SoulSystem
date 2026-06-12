//! Embedder — Moteurs d'embedding pluggables pour la recherche vectorielle.
//!
//! Factorisé depuis soul-memory. Utilise une projection aleatoire deterministe
//! (SciRustEmbedder, 64-dim) ou n-grammes positionnels.

use std::collections::HashMap;

// ── Embedder trait ──────────────────────────────────────────────────────

/// Trait pour les moteurs d'embedding pluggables.
pub trait Embedder: Send + Sync {
    fn embed(&self, text: &str) -> Vec<f32>;
    fn dim(&self) -> usize;
    fn cosine_similarity(&self, a: &[f32], b: &[f32]) -> f32 {
        cosine_similarity(a, b)
    }
}

/// Cosine similarity entre deux vecteurs f32.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}

// ── SciRustEmbedder: projection aleatoire deterministe ──────────────────

/// Embedder base sur une projection aleatoire deterministe (Johnson-Lindenstrauss).
///
/// Utilise 8 fonctions de hachage par n-gramme pour construire un vecteur
/// dense 64-dim. Equivalent a une matrice de projection aleatoire fixe,
/// preservant la similarite cosinus.
pub struct SciRustEmbedder {
    dim: usize,
    seeds: [u64; 8],
}

impl SciRustEmbedder {
    pub fn new(dim: usize) -> Self {
        Self {
            dim,
            seeds: [42, 137, 251, 491, 773, 1021, 1301, 1607],
        }
    }
}

impl Embedder for SciRustEmbedder {
    fn dim(&self) -> usize {
        self.dim
    }

    fn embed(&self, text: &str) -> Vec<f32> {
        if text.is_empty() {
            return vec![0.0; self.dim];
        }

        let chars: Vec<char> = text.chars().collect();
        let mut vec = vec![0.0f32; self.dim];

        for n in 2..=4usize {
            if n > chars.len() {
                continue;
            }
            for i in 0..=(chars.len() - n) {
                let ngram: String = chars[i..i + n].iter().collect();
                let base_hash = seahash::hash(ngram.as_bytes());
                let pos_weight = 1.0 + (i as f32 / chars.len().max(1) as f32) * 0.5;

                for (slot, &seed) in self.seeds.iter().enumerate() {
                    let h = seahash::hash(&base_hash.to_le_bytes());
                    let h2 = seahash::hash(&[seed.to_le_bytes(), h.to_le_bytes()].concat());
                    let idx = (h2 as usize) % self.dim;
                    let sign = if (slot & 1) == 0 { 1.0 } else { -1.0 };
                    vec[idx] += sign * pos_weight;
                }
            }
        }

        l2_normalize(&mut vec);
        vec
    }
}

// ── NGramEmbedder ──────────────────────────────────────────────────────

/// Embedder utilisant n-grammes + hachage positionnel.
pub struct NGramEmbedder {
    min_n: usize,
    max_n: usize,
    dim: usize,
}

impl NGramEmbedder {
    pub fn new(dim: usize) -> Self {
        Self {
            min_n: 2,
            max_n: 4,
            dim,
        }
    }
}

impl Embedder for NGramEmbedder {
    fn dim(&self) -> usize {
        self.dim
    }

    fn embed(&self, text: &str) -> Vec<f32> {
        if text.is_empty() {
            return vec![0.0; self.dim];
        }

        let mut vec = vec![0.0f32; self.dim];
        let chars: Vec<char> = text.chars().collect();

        for n in self.min_n..=self.max_n {
            if n > chars.len() {
                continue;
            }
            for i in 0..=(chars.len() - n) {
                let ngram: String = chars[i..i + n].iter().collect();
                let h = seahash::hash(ngram.as_bytes());
                let pos = (h as usize) % self.dim;
                let pos_weight = 1.0 + (i as f32 / chars.len().max(1) as f32) * 0.5;
                vec[pos] += pos_weight;
            }
        }

        l2_normalize(&mut vec);
        vec
    }
}

fn l2_normalize(v: &mut [f32]) {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        let inv = 1.0 / norm;
        for x in v.iter_mut() {
            *x *= inv;
        }
    }
}

// ── Importance heuristique ─────────────────────────────────────────────

/// Calcule une importance initiale basee sur une heuristique simple.
pub fn compute_initial_importance(text: &str, metadata: &HashMap<String, String>) -> f32 {
    let mut score = 0.5f32;

    let len = text.len() as f32;
    score += (len / 500.0).min(0.2);

    let keywords = [
        "important",
        "critique",
        "urgent",
        "securite",
        "bug",
        "fix",
        "erreur",
        "failure",
        "security",
        "vulnerability",
        " CVE",
        "exploit",
        "architecture",
        "design",
        "decision",
        "breaking",
    ];
    let lower = text.to_lowercase();
    for kw in &keywords {
        if lower.contains(kw) {
            score += 0.05;
        }
    }

    if let Some(source) = metadata.get("source") {
        match source.as_str() {
            "audit" | "security" | "incident" => score += 0.2,
            "arxiv" | "research" => score += 0.1,
            "user_feedback" => score += 0.15,
            _ => {}
        }
    }

    if metadata.get("tag").map(|t| t.as_str()) == Some("veille") {
        score += 0.1;
    }

    score.clamp(0.1, 1.0)
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scirust_embedder_deterministic() {
        let e = SciRustEmbedder::new(64);
        let v1 = e.embed("hello world");
        let v2 = e.embed("hello world");
        assert_eq!(v1.len(), 64);
        assert!(v1.iter().zip(v2.iter()).all(|(a, b)| (a - b).abs() < 1e-6));
    }

    #[test]
    fn test_scirust_embedder_similar_texts_closer() {
        let e = SciRustEmbedder::new(64);
        let v_similar1 = e.embed("machine learning is great");
        let v_similar2 = e.embed("machine learning is awesome");
        let v_different = e.embed("the cat sat on the mat");

        let sim_similar = e.cosine_similarity(&v_similar1, &v_similar2);
        let sim_different = e.cosine_similarity(&v_similar1, &v_different);

        assert!(
            sim_similar > sim_different,
            "similar texts should have higher cosine similarity ({} > {})",
            sim_similar,
            sim_different
        );
    }

    #[test]
    fn test_scirust_embedder_empty() {
        let e = SciRustEmbedder::new(64);
        let v = e.embed("");
        assert_eq!(v.len(), 64);
        assert!(v.iter().all(|x| *x == 0.0));
    }

    #[test]
    fn test_scirust_embedder_normalized() {
        let e = SciRustEmbedder::new(64);
        let v = e.embed("some text for testing normalization");
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!(
            (norm - 1.0).abs() < 1e-5,
            "L2 norm should be ~1.0, got {}",
            norm
        );
    }

    #[test]
    fn test_importance_keywords() {
        let meta = HashMap::new();
        let s1 = compute_initial_importance("hello world", &meta);
        let s2 = compute_initial_importance("critical security vulnerability found", &meta);
        assert!(s2 > s1, "security text should have higher importance");
    }

    #[test]
    fn test_importance_source() {
        let mut meta = HashMap::new();
        meta.insert("source".into(), "audit".into());
        let s = compute_initial_importance("some text", &meta);
        assert!(s > 0.6, "audit source should boost importance, got {}", s);
    }

    #[test]
    fn test_importance_clamped() {
        let meta = HashMap::new();
        let s = compute_initial_importance("", &meta);
        assert!(
            (0.1..=1.0).contains(&s),
            "importance should be clamped to [0.1, 1.0]"
        );
    }
}
