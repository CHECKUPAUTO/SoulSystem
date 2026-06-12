//! Core genetic algorithm implementation.

use std::collections::HashMap;
use std::fmt;

// ---------------------------------------------------------------------------
// PRNG (simple xorshift, no external deps)
// ---------------------------------------------------------------------------

pub(crate) struct Rng(u64);

impl Rng {
    pub fn new() -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .subsec_nanos() as u64;
        Self(if seed == 0 { 1 } else { seed })
    }

    pub fn next_f64(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }

    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + self.next_f64() * (hi - lo)
    }

    pub fn usize_range(&mut self, lo: usize, hi: usize) -> usize {
        if hi <= lo { return lo; }
        lo + (self.next_f64() * (hi - lo) as f64) as usize
    }
}

// ---------------------------------------------------------------------------
// Gene definition
// ---------------------------------------------------------------------------

/// Defines the type and bounds of a gene.
#[derive(Debug, Clone)]
pub enum Gene {
    /// Continuous value in [min, max]
    Continuous { min: f64, max: f64 },
    /// Discrete categorical choice
    Discrete { choices: Vec<f64> },
    /// Binary (0.0 or 1.0)
    Binary,
}

impl Gene {
    pub fn continuous(min: f64, max: f64) -> Self {
        Gene::Continuous { min, max }
    }

    pub fn discrete(choices: Vec<f64>) -> Self {
        Gene::Discrete { choices }
    }

    pub fn binary() -> Self {
        Gene::Binary
    }

    /// Generate a random value within this gene's domain.
    pub fn random(&self, rng: &mut Rng) -> f64 {
        match self {
            Gene::Continuous { min, max } => rng.range(*min, *max),
            Gene::Discrete { choices } => {
                choices[rng.usize_range(0, choices.len())]
            }
            Gene::Binary => {
                if rng.next_f64() < 0.5 { 0.0 } else { 1.0 }
            }
        }
    }

    /// Mutate a value within this gene's domain.
    pub fn mutate(&self, value: f64, rate: f64, rng: &mut Rng) -> f64 {
        if rng.next_f64() >= rate { return value; }
        match self {
            Gene::Continuous { min, max } => {
                let range = max - min;
                let perturbation = rng.range(-0.1 * range, 0.1 * range);
                (value + perturbation).clamp(*min, *max)
            }
            Gene::Discrete { choices } => {
                choices[rng.usize_range(0, choices.len())]
            }
            Gene::Binary => {
                if value < 0.5 { 1.0 } else { 0.0 }
            }
        }
    }

    /// Crossover two parent values.
    pub fn crossover(&self, a: f64, b: f64, rng: &mut Rng) -> f64 {
        match self {
            Gene::Continuous { .. } => {
                if rng.next_f64() < 0.5 { a } else { b }
            }
            Gene::Discrete { .. } => {
                if rng.next_f64() < 0.5 { a } else { b }
            }
            Gene::Binary => {
                if rng.next_f64() < 0.5 { a } else { b }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Individual
// ---------------------------------------------------------------------------

/// A single candidate solution in the population.
#[derive(Debug, Clone)]
pub struct Individual {
    pub genes: HashMap<String, f64>,
    pub fitness: f64,
}

impl Individual {
    pub fn new() -> Self {
        Self { genes: HashMap::new(), fitness: f64::NEG_INFINITY }
    }

    /// Access a gene value by name.
    pub fn get(&self, name: &str) -> f64 {
        self.genes[name]
    }
}

impl std::ops::Index<&str> for Individual {
    type Output = f64;
    fn index(&self, key: &str) -> &f64 {
        &self.genes[key]
    }
}

impl fmt::Display for Individual {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Individual(fit={:.4}, genes={:?})", self.fitness, self.genes)
    }
}

// ---------------------------------------------------------------------------
// Generation statistics
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct GenerationStats {
    pub generation: usize,
    pub best_fitness: f64,
    pub avg_fitness: f64,
    pub worst_fitness: f64,
    pub diversity: f64,
}

// ---------------------------------------------------------------------------
// Genetic Algorithm
// ---------------------------------------------------------------------------

/// Main genetic algorithm engine.
pub struct GA {
    population_size: usize,
    mutation_rate: f64,
    crossover_rate: f64,
    genes: Vec<(String, Gene)>,
    fitness_fn: Option<Box<dyn Fn(&Individual) -> f64>>,
    rng: Rng,
}

impl GA {
    /// Create a new GA with the given population size and mutation rate.
    pub fn new(population_size: usize, mutation_rate: f64) -> Self {
        Self {
            population_size,
            mutation_rate,
            crossover_rate: 0.7,
            genes: Vec::new(),
            fitness_fn: None,
            rng: Rng::new(),
        }
    }

    /// Set crossover rate (default 0.7).
    pub fn with_crossover_rate(mut self, rate: f64) -> Self {
        self.crossover_rate = rate;
        self
    }

    /// Add a gene to the genome.
    pub fn add_gene(&mut self, name: &str, gene: Gene) {
        self.genes.push((name.to_string(), gene));
    }

    /// Set the fitness function.
    pub fn fitness_fn<F: Fn(&Individual) -> f64 + 'static>(&mut self, f: F) {
        self.fitness_fn = Some(Box::new(f));
    }

    /// Create a random individual.
    fn random_individual(&mut self) -> Individual {
        let mut ind = Individual::new();
        for (name, gene) in &self.genes {
            ind.genes.insert(name.clone(), gene.random(&mut self.rng));
        }
        ind
    }

    /// Evaluate fitness for an individual.
    fn evaluate(&self, ind: &Individual) -> f64 {
        match &self.fitness_fn {
            Some(f) => f(ind),
            None => 0.0,
        }
    }

    /// Tournament selection: pick the best of `k` random individuals.
    fn select(&mut self, population: &[Individual], k: usize) -> Individual {
        let mut best_idx = self.rng.usize_range(0, population.len());
        for _ in 1..k {
            let idx = self.rng.usize_range(0, population.len());
            if population[idx].fitness > population[best_idx].fitness {
                best_idx = idx;
            }
        }
        population[best_idx].clone()
    }

    /// Crossover two parents to produce a child.
    fn crossover(&mut self, a: &Individual, b: &Individual) -> Individual {
        let mut child = Individual::new();
        for (name, gene) in &self.genes {
            if self.rng.next_f64() < self.crossover_rate {
                let av = a.genes.get(name).copied().unwrap_or(0.0);
                let bv = b.genes.get(name).copied().unwrap_or(0.0);
                child.genes.insert(name.clone(), gene.crossover(av, bv, &mut self.rng));
            } else {
                child.genes.insert(name.clone(),
                    a.genes.get(name).copied().unwrap_or(0.0));
            }
        }
        child
    }

    /// Mutate an individual in-place.
    fn mutate(&mut self, ind: &mut Individual) {
        for (name, gene) in &self.genes {
            let old = ind.genes.get(name).copied().unwrap_or(0.0);
            ind.genes.insert(name.clone(), gene.mutate(old, self.mutation_rate, &mut self.rng));
        }
    }

    /// Run the GA for `generations` and return the best individual.
    pub fn evolve(&mut self, generations: usize) -> Individual {
        // Initialize population
        let mut population: Vec<Individual> = (0..self.population_size)
            .map(|_| self.random_individual())
            .collect();

        // Evaluate
        for ind in &mut population {
            ind.fitness = self.evaluate(ind);
        }

        let mut best = population.iter()
            .max_by(|a, b| a.fitness.partial_cmp(&b.fitness).unwrap())
            .cloned()
            .unwrap();

        for _gen in 0..generations {
            let mut new_population = Vec::with_capacity(self.population_size);

            // Elitism: keep top 2
            population.sort_by(|a, b| b.fitness.partial_cmp(&a.fitness).unwrap());
            new_population.push(population[0].clone());
            new_population.push(population[1].clone());

            // Fill rest with offspring
            while new_population.len() < self.population_size {
                let parent_a = self.select(&population, 3);
                let parent_b = self.select(&population, 3);
                let mut child = self.crossover(&parent_a, &parent_b);
                self.mutate(&mut child);
                child.fitness = self.evaluate(&child);
                new_population.push(child);
            }

            population = new_population;

            // Track best
            if let Some(current_best) = population.iter()
                .max_by(|a, b| a.fitness.partial_cmp(&b.fitness).unwrap()) {
                if current_best.fitness > best.fitness {
                    best = current_best.clone();
                }
            }
        }

        best
    }

    /// Run the GA and return per-generation statistics.
    pub fn run_with_stats(&mut self, generations: usize) -> Vec<GenerationStats> {
        let mut population: Vec<Individual> = (0..self.population_size)
            .map(|_| self.random_individual())
            .collect();

        for ind in &mut population {
            ind.fitness = self.evaluate(ind);
        }

        let mut stats = Vec::new();

        for gen in 0..generations {
            let mut new_population = Vec::with_capacity(self.population_size);
            population.sort_by(|a, b| b.fitness.partial_cmp(&a.fitness).unwrap());

            // Compute stats
            let best_fit = population[0].fitness;
            let worst_fit = population.last().unwrap().fitness;
            let avg_fit: f64 = population.iter().map(|i| i.fitness).sum::<f64>() / population.len() as f64;

            // Diversity: standard deviation of fitness values
            let variance: f64 = population.iter()
                .map(|i| (i.fitness - avg_fit).powi(2))
                .sum::<f64>() / population.len() as f64;
            let diversity = variance.sqrt();

            stats.push(GenerationStats {
                generation: gen,
                best_fitness: best_fit,
                avg_fitness: avg_fit,
                worst_fitness: worst_fit,
                diversity,
            });

            // Elitism
            new_population.push(population[0].clone());
            new_population.push(population[1].clone());

            while new_population.len() < self.population_size {
                let parent_a = self.select(&population, 3);
                let parent_b = self.select(&population, 3);
                let mut child = self.crossover(&parent_a, &parent_b);
                self.mutate(&mut child);
                child.fitness = self.evaluate(&child);
                new_population.push(child);
            }

            population = new_population;
        }

        stats
    }
}
