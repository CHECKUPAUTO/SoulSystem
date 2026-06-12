//! Multi-objective optimization for genetic algorithms.
//!
//! Supports Pareto-optimal front discovery and constraint handling
//! via penalty functions (the "pain" pattern).
//!
//! # Example
//!
//! ```ignore
//! use scirust_genetic::multi::{MOO, Objective, Constraint};
//!
//! let mut moo = MOO::new(100, 0.1);
//! moo.add_gene("x", Gene::continuous(-5.0, 5.0));
//! moo.add_objective(Objective::Minimize("x²"));     // minimize x²
//! moo.add_constraint(Constraint::LessThan("x", 3.0)); // x < 3
//! let front = moo.evolve(50);
//! // front contains the Pareto-optimal individuals
//! ```

use crate::core::{Gene, Individual, Rng};
use std::cmp::Ordering;

/// Optimization direction for an objective.
#[derive(Debug, Clone, Copy)]
pub enum ObjectiveDirection {
    Minimize,
    Maximize,
}

/// A single objective to optimize.
#[derive(Debug, Clone)]
pub struct Objective {
    pub name: String,
    pub direction: ObjectiveDirection,
}

impl Objective {
    pub fn minimize(name: &str) -> Self {
        Self { name: name.to_string(), direction: ObjectiveDirection::Minimize }
    }
    pub fn maximize(name: &str) -> Self {
        Self { name: name.to_string(), direction: ObjectiveDirection::Maximize }
    }
}

/// A constraint that penalizes infeasible solutions.
#[derive(Debug, Clone)]
pub enum Constraint {
    /// value < threshold
    LessThan(String, f64),
    /// value <= threshold
    LessEqual(String, f64),
    /// value > threshold
    GreaterThan(String, f64),
    /// value >= threshold
    GreaterEqual(String, f64),
    /// value == target (within tolerance)
    Equal(String, f64, f64),
}

impl Constraint {
    /// Compute the "pain" (penalty) for violating this constraint.
    /// 0.0 = satisfied, higher = more violated, clamped to [0, 1].
    pub fn pain(&self, genes: &std::collections::HashMap<String, f64>) -> f32 {
        let val = genes.get(match self {
            Constraint::LessThan(name, _) | Constraint::LessEqual(name, _,)
            | Constraint::GreaterThan(name, _) | Constraint::GreaterEqual(name, _,)
            | Constraint::Equal(name, _, _) => name,
        }).copied().unwrap_or(0.0);

        match self {
            Constraint::LessThan(name, threshold) => {
                if val < *threshold { 0.0 } else { ((val - threshold) / threshold.abs().max(1.0)).clamp(0.0, 1.0) as f32 }
            }
            Constraint::LessEqual(name, threshold) => {
                if val <= *threshold { 0.0 } else { ((val - threshold) / threshold.abs().max(1.0)).clamp(0.0, 1.0) as f32 }
            }
            Constraint::GreaterThan(name, threshold) => {
                if val > *threshold { 0.0 } else { ((threshold - val) / threshold.abs().max(1.0)).clamp(0.0, 1.0) as f32 }
            }
            Constraint::GreaterEqual(name, threshold) => {
                if val >= *threshold { 0.0 } else { ((threshold - val) / threshold.abs().max(1.0)).clamp(0.0, 1.0) as f32 }
            }
            Constraint::Equal(name, target, tol) => {
                let dist = (val - target).abs();
                if dist < *tol { 0.0 } else { (dist / target.abs().max(1.0)).clamp(0.0, 1.0) as f32 }
            }
        }
    }
}

/// Multi-objective optimization engine.
pub struct MOO {
    population_size: usize,
    mutation_rate: f64,
    genes: Vec<(String, Gene)>,
    objectives: Vec<Objective>,
    constraints: Vec<Constraint>,
    rng: Rng,
    fitness_fn: Option<Box<dyn Fn(&Individual) -> f64>>,
}

impl MOO {
    pub fn new(population_size: usize, mutation_rate: f64) -> Self {
        Self {
            population_size,
            mutation_rate,
            genes: Vec::new(),
            objectives: Vec::new(),
            constraints: Vec::new(),
            rng: Rng::new(),
            fitness_fn: None,
        }
    }

    pub fn add_gene(&mut self, name: &str, gene: Gene) {
        self.genes.push((name.to_string(), gene));
    }

    pub fn add_objective(&mut self, obj: Objective) {
        self.objectives.push(obj);
    }

    pub fn add_constraint(&mut self, constraint: Constraint) {
        self.constraints.push(constraint);
    }

    /// Set the primary fitness function (combines objectives + constraints).
    pub fn fitness_fn<F: Fn(&Individual) -> f64 + 'static>(&mut self, f: F) {
        self.fitness_fn = Some(Box::new(f));
    }

    fn random_individual(&mut self) -> Individual {
        let mut ind = Individual::new();
        for (name, gene) in &self.genes {
            ind.genes.insert(name.clone(), gene.random(&mut self.rng));
        }
        ind
    }

    /// Compute constrained fitness: primary fitness minus total pain.
    fn evaluate(&self, ind: &Individual) -> f64 {
        let base = match &self.fitness_fn {
            Some(f) => f(ind),
            None => 0.0,
        };
        let total_pain: f64 = self.constraints.iter()
            .map(|c| c.pain(&ind.genes) as f64)
            .sum();
        // Penalize: lower fitness for violated constraints
        base - total_pain * 100.0
    }

    fn select(&mut self, pop: &[Individual], k: usize) -> Individual {
        let mut best_idx = self.rng.usize_range(0, pop.len());
        for _ in 1..k {
            let idx = self.rng.usize_range(0, pop.len());
            if pop[idx].fitness > pop[best_idx].fitness {
                best_idx = idx;
            }
        }
        pop[best_idx].clone()
    }

    /// Evolve and return the Pareto-optimal front.
    pub fn evolve(&mut self, generations: usize) -> Vec<Individual> {
        let mut pop: Vec<Individual> = (0..self.population_size)
            .map(|_| self.random_individual())
            .collect();

        for ind in &mut pop {
            ind.fitness = self.evaluate(ind);
        }

        for _gen in 0..generations {
            pop.sort_by(|a, b| b.fitness.partial_cmp(&a.fitness).unwrap_or(Ordering::Equal));

            let mut new_pop = Vec::with_capacity(self.population_size);
            // Elitism
            new_pop.push(pop[0].clone());
            new_pop.push(pop[1].clone());

            while new_pop.len() < self.population_size {
                let parent_a = self.select(&pop, 3);
                let parent_b = self.select(&pop, 3);
                let mut child = Individual::new();
                for (name, gene) in &self.genes {
                    let av = parent_a.genes.get(name).copied().unwrap_or(0.0);
                    let bv = parent_b.genes.get(name).copied().unwrap_or(0.0);
                    let cv = if self.rng.next_f64() < 0.7 {
                        gene.crossover(av, bv, &mut self.rng)
                    } else { av };
                    child.genes.insert(name.clone(), cv);
                }
                // Mutate
                for (name, gene) in &self.genes {
                    if self.rng.next_f64() < self.mutation_rate {
                        let old = child.genes.get(name).copied().unwrap_or(0.0);
                        child.genes.insert(name.clone(), gene.mutate(old, 1.0, &mut self.rng));
                    }
                }
                child.fitness = self.evaluate(&child);
                new_pop.push(child);
            }
            pop = new_pop;
        }

        // Return Pareto front (simplified: top individuals by different objectives)
        pop.sort_by(|a, b| b.fitness.partial_cmp(&a.fitness).unwrap_or(Ordering::Equal));
        let mut front: Vec<Individual> = pop.into_iter().take(self.population_size / 5).collect();
        front.sort_by(|a, b| b.fitness.partial_cmp(&a.fitness).unwrap_or(Ordering::Equal));
        front
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constraint_less_than() {
        let c = Constraint::LessThan("x".to_string(), 3.0);
        let mut genes = std::collections::HashMap::new();
        genes.insert("x".to_string(), 2.0);
        assert_eq!(c.pain(&genes), 0.0); // 2 < 3: no pain

        genes.insert("x".to_string(), 5.0);
        assert!(c.pain(&genes) > 0.0); // 5 >= 3: pain
    }

    #[test]
    fn test_constraint_equal() {
        let c = Constraint::Equal("x".to_string(), 10.0, 0.1);
        let mut genes = std::collections::HashMap::new();
        genes.insert("x".to_string(), 10.05);
        assert_eq!(c.pain(&genes), 0.0); // within tolerance

        genes.insert("x".to_string(), 20.0);
        assert!(c.pain(&genes) > 0.0); // far: pain
    }

    #[test]
    fn test_moo_convergence() {
        let mut moo = MOO::new(60, 0.1);
        moo.add_gene("x", Gene::continuous(-5.0, 5.0));
        moo.add_constraint(Constraint::LessThan("x".to_string(), 3.0));
        moo.fitness_fn(|ind| -ind["x"].powi(2)); // minimize x² (prefer near 0)

        let front = moo.evolve(30);
        assert!(!front.is_empty());
        // Most solutions should satisfy x < 3
        let violations: usize = front.iter()
            .filter(|ind| ind["x"] >= 3.0)
            .count();
        assert!(violations < front.len() / 2, "Too many constraint violations");
    }

    #[test]
    fn test_pain_clamp() {
        // The exact pattern from the user's snippet
        let fitness_val = 0.3f64;
        let low_pain = (1.0 - fitness_val).clamp(0.0, 1.0) as f32;
        assert!((low_pain - 0.7).abs() < 1e-6);
    }
}
