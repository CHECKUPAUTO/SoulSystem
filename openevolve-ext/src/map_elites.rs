//! Map-Elites quality-diversity archive.
//!
//! Where NSGA-II keeps a Pareto front, Map-Elites keeps the **best program in
//! each behavioural cell**. Cells are defined by a low-dimensional
//! "behaviour descriptor" — here a triple of buckets over
//! (complexity_class, loc_bucket, function_count_bucket).
//!
//! Map-Elites is excellent at escaping local optima: many distinct strategies
//! coexist in different cells instead of being culled by domination.

use crate::program::Program;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

/// 3D behaviour descriptor. All values are small non-negative integers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BehaviourKey {
    pub complexity_class: u8, // 0..=7  (analysis::ComplexityClass as u8)
    pub loc_bucket: u8,       // 0..=9  log-scale buckets
    pub fn_bucket: u8,        // 0..=5  function-count buckets
}

impl BehaviourKey {
    pub fn from_program(p: &Program) -> Self {
        let cc = p.metrics.get("complexity_class").copied().unwrap_or(0.0) as u8;
        let loc = p.metrics.get("loc").copied().unwrap_or(0.0) as u32;
        let nfn = p.metrics.get("function_count").copied().unwrap_or(0.0) as u32;
        Self {
            complexity_class: cc.min(7),
            loc_bucket: loc_bucket(loc),
            fn_bucket: fn_bucket(nfn),
        }
    }
}

fn loc_bucket(loc: u32) -> u8 {
    match loc {
        0..=10 => 0,
        11..=25 => 1,
        26..=50 => 2,
        51..=100 => 3,
        101..=200 => 4,
        201..=400 => 5,
        401..=800 => 6,
        801..=1500 => 7,
        1501..=3000 => 8,
        _ => 9,
    }
}

fn fn_bucket(n: u32) -> u8 {
    match n {
        0..=1 => 0,
        2..=3 => 1,
        4..=7 => 2,
        8..=15 => 3,
        16..=31 => 4,
        _ => 5,
    }
}

#[derive(Debug, Default)]
pub struct MapElites {
    cells: IndexMap<BehaviourKey, Program>,
}

impl MapElites {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert `program` in its cell if it scores higher than the current elite.
    /// Returns `true` when the archive accepted the insertion.
    pub fn insert(&mut self, program: Program) -> bool {
        let key = BehaviourKey::from_program(&program);
        match self.cells.get(&key) {
            Some(existing) if existing.score >= program.score => false,
            _ => {
                self.cells.insert(key, program);
                true
            }
        }
    }

    pub fn len(&self) -> usize {
        self.cells.len()
    }
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    pub fn best(&self) -> Option<&Program> {
        self.cells.values().max_by(|a, b| {
            a.score
                .partial_cmp(&b.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    }

    pub fn coverage(&self) -> usize {
        self.cells.len()
    }

    /// Mean score across all cells — a rough "quality of coverage" indicator.
    pub fn mean_score(&self) -> f64 {
        if self.cells.is_empty() {
            return 0.0;
        }
        self.cells.values().map(|p| p.score).sum::<f64>() / self.cells.len() as f64
    }

    /// Sample a cell uniformly at random; useful for diversifying parent picks.
    pub fn sample_cell(&self, rng: &mut impl rand::Rng) -> Option<&Program> {
        if self.cells.is_empty() {
            return None;
        }
        let idx = rng.gen_range(0..self.cells.len());
        self.cells.get_index(idx).map(|(_, p)| p)
    }

    pub fn cells(&self) -> impl Iterator<Item = (&BehaviourKey, &Program)> {
        self.cells.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make(score: f64, cc: f64, loc: f64, fns: f64) -> Program {
        let mut p = Program::new("x".into(), 0, None, 0);
        p.score = score;
        p.metrics.insert("complexity_class".into(), cc);
        p.metrics.insert("loc".into(), loc);
        p.metrics.insert("function_count".into(), fns);
        p
    }

    #[test]
    fn insert_higher_score_wins_cell() {
        let mut m = MapElites::new();
        assert!(m.insert(make(0.4, 1.0, 20.0, 2.0)));
        assert!(m.insert(make(0.7, 1.0, 20.0, 2.0))); // same cell, higher
        assert!(!m.insert(make(0.5, 1.0, 20.0, 2.0))); // lower, rejected
        assert_eq!(m.len(), 1);
        assert!((m.best().unwrap().score - 0.7).abs() < 1e-9);
    }

    #[test]
    fn different_cells_coexist() {
        let mut m = MapElites::new();
        assert!(m.insert(make(0.4, 1.0, 20.0, 2.0)));
        assert!(m.insert(make(0.4, 4.0, 200.0, 8.0)));
        assert_eq!(m.len(), 2);
    }

    #[test]
    fn sample_cell_returns_something() {
        let mut m = MapElites::new();
        m.insert(make(0.5, 1.0, 20.0, 2.0));
        let mut rng = rand::thread_rng();
        assert!(m.sample_cell(&mut rng).is_some());
    }
}
