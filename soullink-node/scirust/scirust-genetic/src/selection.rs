//! Selection strategies for genetic algorithms.

use crate::core::{Individual, Rng};

/// Selection method for choosing parents.
#[derive(Debug, Clone, Copy)]
pub enum SelectionMethod {
    /// Tournament selection of size k
    Tournament(usize),
    /// Roulette wheel (fitness-proportional)
    Roulette,
    /// Rank-based selection
    Rank,
}

/// Select a parent from the population using the given method.
pub fn select(method: &SelectionMethod, population: &[Individual], rng: &mut Rng) -> Individual {
    match method {
        SelectionMethod::Tournament(k) => {
            let k = (*k).min(population.len());
            let mut best_idx = rng.usize_range(0, population.len());
            for _ in 1..k {
                let idx = rng.usize_range(0, population.len());
                if population[idx].fitness > population[best_idx].fitness {
                    best_idx = idx;
                }
            }
            population[best_idx].clone()
        }
        SelectionMethod::Roulette => {
            let total_fitness: f64 = population.iter()
                .map(|i| (i.fitness - population.iter().map(|x| x.fitness).min_by(|a, b| a.partial_cmp(b).unwrap()).unwrap_or(0.0) + 1.0).max(0.0))
                .sum();

            if total_fitness <= 0.0 {
                return population[rng.usize_range(0, population.len())].clone();
            }

            let mut spin = rng.next_f64() * total_fitness;
            for ind in population {
                let adjusted = (ind.fitness - population.iter().map(|x| x.fitness).min_by(|a, b| a.partial_cmp(b).unwrap()).unwrap_or(0.0) + 1.0).max(0.0);
                spin -= adjusted;
                if spin <= 0.0 {
                    return ind.clone();
                }
            }
            population.last().unwrap().clone()
        }
        SelectionMethod::Rank => {
            let mut sorted: Vec<&Individual> = population.iter().collect();
            sorted.sort_by(|a, b| a.fitness.partial_cmp(&b.fitness).unwrap());

            let n = sorted.len();
            let total_rank: usize = n * (n + 1) / 2;
            let mut spin = rng.next_f64() * total_rank as f64;

            for (i, ind) in sorted.iter().enumerate() {
                spin -= (i + 1) as f64;
                if spin <= 0.0 {
                    return (*ind).clone();
                }
            }
            (*sorted.last().unwrap()).clone()
        }
    }
}
