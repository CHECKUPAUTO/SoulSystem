//! NSGA-II multi-objective selection over (performance, readability, size, security).
//!
//! Programs expose four objectives derived from `Program.metrics`:
//!   - performance : `program.score`
//!   - readability : `metrics["static_quality"]`  (from analysis::analyze)
//!   - size        : `1 / (1 + loc/100)`           (from analysis::analyze)
//!   - security    : `1 - 0.1 * #security_warnings` (clamped)
//!
//! All objectives are maximised, all clamped to `[0, 1]`.
//!
//! `ParetoFront` keeps a non-dominated archive bounded by `max_size`, breaking
//! ties on score. `nsga2_select` performs a full NSGA-II selection round.

use crate::program::Program;
use std::cmp::Ordering;

#[derive(Debug, Clone, PartialEq)]
pub struct MultiObjective {
    pub performance: f64,
    pub readability: f64,
    pub size: f64,
    pub security: f64,
}

impl MultiObjective {
    pub fn from_program(p: &Program) -> Self {
        let m = &p.metrics;
        let performance = p.score.clamp(0.0, 1.0);
        let readability = m
            .get("static_quality")
            .copied()
            .unwrap_or(0.5)
            .clamp(0.0, 1.0);
        let loc = m.get("loc").copied().unwrap_or(100.0).max(0.0);
        let size = 1.0 / (1.0 + loc / 100.0);
        let sec_count = m.get("security_warnings").copied().unwrap_or(0.0).max(0.0);
        let security = (1.0 - 0.10 * sec_count).clamp(0.0, 1.0);
        Self {
            performance,
            readability,
            size,
            security,
        }
    }

    /// `self` dominates `other` iff it is >= on all four objectives and > on
    /// at least one.
    pub fn dominates(&self, other: &Self) -> bool {
        let ge = self.performance >= other.performance
            && self.readability >= other.readability
            && self.size >= other.size
            && self.security >= other.security;
        let gt = self.performance > other.performance
            || self.readability > other.readability
            || self.size > other.size
            || self.security > other.security;
        ge && gt
    }

    pub fn value(&self, idx: usize) -> f64 {
        match idx {
            0 => self.performance,
            1 => self.readability,
            2 => self.size,
            3 => self.security,
            _ => 0.0,
        }
    }
}

// ── ParetoFront archive ─────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ParetoFront {
    max_size: usize,
    members: Vec<Program>,
}

impl ParetoFront {
    pub fn new(max_size: usize) -> Self {
        Self {
            max_size: max_size.max(1),
            members: Vec::new(),
        }
    }

    /// Try to insert `candidate`. Returns true if it entered the front.
    pub fn insert(&mut self, candidate: Program) -> bool {
        let cand_obj = MultiObjective::from_program(&candidate);

        // If the candidate is dominated by any existing member, reject.
        if self
            .members
            .iter()
            .any(|m| MultiObjective::from_program(m).dominates(&cand_obj))
        {
            return false;
        }

        // Remove existing members that the candidate dominates.
        self.members.retain(|m| {
            let mo = MultiObjective::from_program(m);
            !cand_obj.dominates(&mo)
        });

        self.members.push(candidate);
        self.members
            .sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal));
        if self.members.len() > self.max_size {
            self.members.pop();
        }
        true
    }

    pub fn members(&self) -> &[Program] {
        &self.members
    }
    pub fn best(&self) -> Option<&Program> {
        self.members.first()
    }
    pub fn size(&self) -> usize {
        self.members.len()
    }
    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }
}

// ── NSGA-II ────────────────────────────────────────────────────────────

/// Fast non-dominated sort. Returns ordered fronts as `Vec<Vec<index>>`.
pub fn fast_non_dominated_sort(objs: &[MultiObjective]) -> Vec<Vec<usize>> {
    let n = objs.len();
    if n == 0 {
        return Vec::new();
    }
    let mut dom_count = vec![0usize; n];
    let mut dom_set: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut first_front = Vec::new();

    for p in 0..n {
        for q in 0..n {
            if p == q {
                continue;
            }
            if objs[p].dominates(&objs[q]) {
                dom_set[p].push(q);
            } else if objs[q].dominates(&objs[p]) {
                dom_count[p] += 1;
            }
        }
        if dom_count[p] == 0 {
            first_front.push(p);
        }
    }
    let mut fronts: Vec<Vec<usize>> = vec![first_front];
    let mut i = 0;
    while i < fronts.len() && !fronts[i].is_empty() {
        let mut next = Vec::new();
        for &p in &fronts[i] {
            for &q in &dom_set[p] {
                dom_count[q] = dom_count[q].saturating_sub(1);
                if dom_count[q] == 0 {
                    next.push(q);
                }
            }
        }
        if next.is_empty() {
            break;
        }
        fronts.push(next);
        i += 1;
    }
    fronts
}

/// Per-individual crowding distance. Boundary points get infinity to preserve
/// the spread of the front.
pub fn crowding_distance(objs: &[MultiObjective], fronts: &[Vec<usize>]) -> Vec<f64> {
    let n = objs.len();
    let mut dist = vec![0.0; n];
    for front in fronts {
        if front.len() <= 2 {
            for &i in front {
                dist[i] = f64::INFINITY;
            }
            continue;
        }
        for &i in front {
            dist[i] = 0.0;
        }
        for obj_idx in 0..4 {
            let mut sorted = front.clone();
            sorted.sort_by(|&a, &b| {
                objs[a]
                    .value(obj_idx)
                    .partial_cmp(&objs[b].value(obj_idx))
                    .unwrap_or(Ordering::Equal)
            });
            dist[sorted[0]] = f64::INFINITY;
            dist[sorted[sorted.len() - 1]] = f64::INFINITY;
            let f_min = objs[sorted[0]].value(obj_idx);
            let f_max = objs[sorted[sorted.len() - 1]].value(obj_idx);
            let span = f_max - f_min;
            if span.abs() < f64::EPSILON {
                continue;
            }
            for k in 1..sorted.len() - 1 {
                let prev = objs[sorted[k - 1]].value(obj_idx);
                let next = objs[sorted[k + 1]].value(obj_idx);
                dist[sorted[k]] += (next - prev) / span;
            }
        }
    }
    dist
}

/// Select `n` survivors from `programs` using NSGA-II.
pub fn nsga2_select(programs: &[Program], n: usize) -> Vec<usize> {
    if programs.is_empty() || n == 0 {
        return Vec::new();
    }
    let objs: Vec<MultiObjective> = programs.iter().map(MultiObjective::from_program).collect();
    let fronts = fast_non_dominated_sort(&objs);
    let dist = crowding_distance(&objs, &fronts);

    let mut selected = Vec::with_capacity(n);
    for front in &fronts {
        if selected.len() + front.len() <= n {
            selected.extend_from_slice(front);
        } else {
            let mut sorted = front.clone();
            sorted.sort_by(|&a, &b| dist[b].partial_cmp(&dist[a]).unwrap_or(Ordering::Equal));
            let need = n - selected.len();
            selected.extend_from_slice(&sorted[..need]);
            break;
        }
    }
    selected
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make(score: f64, quality: f64, loc: f64) -> Program {
        let mut p = Program::new("pass".into(), 0, None, 0);
        p.score = score;
        p.metrics.insert("static_quality".into(), quality);
        p.metrics.insert("loc".into(), loc);
        p
    }

    #[test]
    fn dominates_basic() {
        let a = MultiObjective {
            performance: 0.9,
            readability: 0.8,
            size: 0.7,
            security: 0.6,
        };
        let b = MultiObjective {
            performance: 0.8,
            readability: 0.7,
            size: 0.6,
            security: 0.5,
        };
        assert!(a.dominates(&b));
        assert!(!b.dominates(&a));
    }

    #[test]
    fn dominated_candidate_rejected() {
        let mut front = ParetoFront::new(10);
        // dominator: high on every objective
        let dom = make(1.0, 0.9, 10.0);
        // dominated: lower on every objective
        let domd = make(0.5, 0.5, 50.0);
        assert!(front.insert(dom));
        assert!(!front.insert(domd));
        assert_eq!(front.size(), 1);
    }

    #[test]
    fn non_dominated_candidates_coexist() {
        let mut front = ParetoFront::new(10);
        // Two programs that trade off score vs size — neither dominates the other.
        let a = make(0.9, 0.5, 200.0); // high perf, big
        let b = make(0.5, 0.5, 10.0); // low perf, small
        assert!(front.insert(a));
        assert!(front.insert(b));
        assert_eq!(front.size(), 2);
    }

    #[test]
    fn best_returns_highest_score() {
        let mut front = ParetoFront::new(10);
        let a = make(0.5, 0.5, 100.0);
        let b = make(0.8, 0.4, 200.0);
        front.insert(a);
        front.insert(b);
        // both kept (trade-off), best by score is 0.8
        assert!(front.best().unwrap().score >= 0.8 - 1e-9);
    }

    #[test]
    fn nsga2_selects_requested_count() {
        let pop = vec![
            make(0.9, 0.1, 10.0),
            make(0.1, 0.9, 10.0),
            make(0.5, 0.5, 10.0),
        ];
        let sel = nsga2_select(&pop, 2);
        assert_eq!(sel.len(), 2);
    }
}
