//! The open-ended archive of accepted self-modifications.
//!
//! Unlike a hill-climber that only keeps the single best variant, the Darwin
//! Gödel Machine keeps *every* validated stepping stone and may branch from any
//! of them. This preserves diversity and lets the search escape local optima —
//! the same open-endedness argued for by Lehman & Stanley ("Why Greatness
//! Cannot Be Planned", 2015). The archive here is that population.

use crate::rng::SplitMix64;
use crate::variant::{Fitness, Variant};
use serde::{Deserialize, Serialize};

/// Persistent, append-mostly collection of accepted variants.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Archive {
    variants: Vec<Variant>,
}

impl Archive {
    pub fn new() -> Self {
        Self::default()
    }

    /// Seed the archive with the baseline (unmodified) codebase.
    pub fn with_root(baseline: Fitness) -> Self {
        let mut a = Self::new();
        a.variants.push(Variant::root(baseline));
        a
    }

    pub fn len(&self) -> usize {
        self.variants.len()
    }

    pub fn is_empty(&self) -> bool {
        self.variants.is_empty()
    }

    pub fn variants(&self) -> &[Variant] {
        &self.variants
    }

    /// Insert an accepted variant.
    pub fn insert(&mut self, variant: Variant) {
        self.variants.push(variant);
    }

    /// Look a variant up by id.
    pub fn get(&self, id: &str) -> Option<&Variant> {
        self.variants.iter().find(|v| v.id == id)
    }

    /// The single best variant by fitness (the candidate to promote).
    pub fn best(&self) -> Option<&Variant> {
        self.variants
            .iter()
            .max_by(|a, b| match (&a.fitness, &b.fitness) {
                (Some(fa), Some(fb)) => {
                    if fa.is_better_than(fb) {
                        std::cmp::Ordering::Greater
                    } else if fb.is_better_than(fa) {
                        std::cmp::Ordering::Less
                    } else {
                        std::cmp::Ordering::Equal
                    }
                }
                (Some(_), None) => std::cmp::Ordering::Greater,
                (None, Some(_)) => std::cmp::Ordering::Less,
                (None, None) => std::cmp::Ordering::Equal,
            })
    }

    /// Select a parent to branch from.
    ///
    /// Selection is open-ended: every variant has a non-zero chance, but better
    /// variants and *under-explored* lineages (few children so far) are favoured.
    /// This is a lightweight quality-diversity weighting — exploit the good,
    /// keep exploring the rare. Deterministic for a given RNG state.
    pub fn select_parent(&self, rng: &mut SplitMix64) -> Option<&Variant> {
        if self.variants.is_empty() {
            return None;
        }

        // child counts per parent id, for the novelty bonus.
        let weights: Vec<f64> = self
            .variants
            .iter()
            .map(|v| {
                let children = self
                    .variants
                    .iter()
                    .filter(|c| c.parent.as_deref() == Some(v.id.as_str()))
                    .count() as f64;
                // Rank-style quality term in [0,1]; broken roots still get a floor.
                let quality = v
                    .fitness
                    .as_ref()
                    .map(|f| if f.compiles { 1.0 } else { 0.1 })
                    .unwrap_or(0.1);
                // Novelty bonus shrinks as a lineage is explored.
                let novelty = 1.0 / (1.0 + children);
                quality * novelty + f64::EPSILON
            })
            .collect();

        let total: f64 = weights.iter().sum();
        let mut pick = rng.next_f64() * total;
        for (v, w) in self.variants.iter().zip(weights.iter()) {
            pick -= w;
            if pick <= 0.0 {
                return Some(v);
            }
        }
        self.variants.last()
    }

    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json(s: &str) -> serde_json::Result<Self> {
        serde_json::from_str(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::variant::{Patch, Variant};

    fn fit(score: f64) -> Fitness {
        Fitness {
            compiles: true,
            tests_passed: 1,
            tests_failed: 0,
            score,
            notes: String::new(),
        }
    }

    #[test]
    fn root_archive_has_one_entry() {
        let a = Archive::with_root(fit(0.0));
        assert_eq!(a.len(), 1);
        assert!(!a.is_empty());
    }

    #[test]
    fn best_tracks_highest_fitness() {
        let mut a = Archive::with_root(fit(0.0));
        let root = a.variants()[0].clone();
        let mut better = Variant::child(&root, Patch::new("a", "x", "y"), "improve");
        better.fitness = Some(fit(5.0));
        a.insert(better.clone());
        assert_eq!(a.best().unwrap().id, better.id);
    }

    #[test]
    fn parent_selection_is_seed_deterministic() {
        let a = Archive::with_root(fit(0.0));
        let mut r1 = SplitMix64::new(42);
        let mut r2 = SplitMix64::new(42);
        let p1 = a.select_parent(&mut r1).unwrap().id.clone();
        let p2 = a.select_parent(&mut r2).unwrap().id.clone();
        assert_eq!(p1, p2);
    }

    #[test]
    fn json_round_trip() {
        let a = Archive::with_root(fit(1.5));
        let s = a.to_json().unwrap();
        let b = Archive::from_json(&s).unwrap();
        assert_eq!(a.len(), b.len());
    }
}
