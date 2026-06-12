# Adaptive Mutation Rates Guide

**Source:** OpenEvolve Night Cycle Report 2026-04-12 (0019)
**Purpose:** Dynamic mutation rate adjustment in IronReview T430 algorithm based on convergence

## Current Implementation

Fixed mutation rate of 0.1 across all generations:

```rust
// Current: ironreview/src/evolution.rs
const MUTATION_RATE: f64 = 0.1;

fn mutate(individual: &mut Individual) {
    if random() < MUTATION_RATE {
        // Apply mutation
    }
}
```

## Adaptive Implementation

```rust
// Proposed: ironreview/src/evolution.rs

pub struct AdaptiveMutationConfig {
    pub base_rate: f64,
    pub exploration_rate: f64,
    pub exploitation_rate: f64,
    pub diversity_injection_rate: f64,
}

impl Default for AdaptiveMutationConfig {
    fn default() -> Self {
        Self {
            base_rate: 0.1,
            exploration_rate: 0.2,
            exploitation_rate: 0.05,
            diversity_injection_rate: 0.25,
        }
    }
}

pub fn adaptive_mutation_rate(
    generation: usize,
    avg_fitness: f64,
    config: &AdaptiveMutationConfig,
) -> f64 {
    // Exploitation: Low mutation when converging
    if avg_fitness > 0.9 {
        return config.exploitation_rate;
    }
    
    // Exploration: High mutation early
    if generation < 20 {
        return config.exploration_rate;
    }
    
    // Diversity injection: Periodic high mutation
    if generation % 10 == 0 {
        return config.diversity_injection_rate;
    }
    
    // Default
    config.base_rate
}

pub fn mutate_adaptive(
    individual: &mut Individual,
    generation: usize,
    avg_fitness: f64,
) {
    let config = AdaptiveMutationConfig::default();
    let rate = adaptive_mutation_rate(generation, avg_fitness, &config);
    
    if random() < rate {
        apply_mutation(individual);
    }
}
```

## Algorithm Phases

| Phase | Condition | Mutation Rate | Purpose |
|-------|-----------|---------------|---------|
| **Explore** | gen < 20 | 0.20 | Broad search |
| **Stabilize** | 20 ≤ gen < 50 | 0.10 | Refine |
| **Exploit** | avg_fitness > 0.9 | 0.05 | Converge |
| **Diversity** | gen % 10 == 0 | 0.25 | Escape local optima |

## Convergence Detection

```rust
pub struct ConvergenceTracker {
    fitness_history: Vec<f64>,
    window_size: usize,
    threshold: f64,
}

impl ConvergenceTracker {
    pub fn is_converged(&self) -> bool {
        if self.fitness_history.len() < self.window_size {
            return false;
        }
        
        let recent = &self.fitness_history[
            self.fitness_history.len() - self.window_size..
        ];
        
        let variance = calculate_variance(recent);
        variance < self.threshold
    }
    
    pub fn record(&mut self, fitness: f64) {
        self.fitness_history.push(fitness);
    }
}
```

## Integration with T430

```rust
// ironreview/src/t430.rs

pub struct T430Algorithm {
    generation: usize,
    population: Vec<Individual>,
    convergence: ConvergenceTracker,
    mutation_config: AdaptiveMutationConfig,
}

impl T430Algorithm {
    pub fn evolve_generation(&mut self) {
        let avg_fitness = self.calculate_average_fitness();
        self.convergence.record(avg_fitness);
        
        let mutation_rate = adaptive_mutation_rate(
            self.generation,
            avg_fitness,
            &self.mutation_config,
        );
        
        // Apply to offspring generation
        for individual in &mut self.population {
            if random() < mutation_rate {
                self.mutate(individual);
            }
        }
        
        self.generation += 1;
    }
}
```

## Configuration File

```toml
# ironreview.toml
[mutation]
base_rate = 0.1
exploration_rate = 0.2
exploitation_rate = 0.05
diversity_injection_rate = 0.25
early_generations = 20
convergence_threshold = 0.9
diversity_interval = 10
```

## Expected Improvements

| Metric | Fixed Rate | Adaptive | Improvement |
|--------|-----------|----------|-------------|
| Convergence Speed | ~100 gen | ~70 gen | 30% faster |
| Solution Quality | 0.92 | 0.95 | +3% |
| Diversity | Medium | High | Better exploration |

## Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exploration_phase() {
        let rate = adaptive_mutation_rate(10, 0.5, &AdaptiveMutationConfig::default());
        assert_eq!(rate, 0.2); // exploration_rate
    }

    #[test]
    fn test_exploitation_phase() {
        let rate = adaptive_mutation_rate(100, 0.95, &AdaptiveMutationConfig::default());
        assert_eq!(rate, 0.05); // exploitation_rate
    }

    #[test]
    fn test_diversity_injection() {
        let rate = adaptive_mutation_rate(30, 0.8, &AdaptiveMutationConfig::default());
        assert_eq!(rate, 0.25); // diversity_injection_rate
    }
}
```

## References

- Night Cycle Report: night_cycle_20260412_0019.md
- Algorithm: IronReview T430 Phase-Shift
