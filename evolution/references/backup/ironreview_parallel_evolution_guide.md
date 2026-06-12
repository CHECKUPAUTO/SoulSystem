# IronReview Parallel Evolution Guide

## Overview

**Pattern:** Rayon-Based Parallel Fitness Evaluation
**Source:** T430 Algorithm reference in night_cycle_20260412_0019
**Target:** IronReview V4 Rust Implementation
**Priority:** Medium (P2)

## Current Implementation

IronReview V4 uses sequential fitness evaluation:

```rust
// Current (sequential)
let offspring: Vec<Genome> = (0..POP_SIZE)
    .map(|_| crossover_and_mutate(&parents))
    .collect();

// Sequential fitness evaluation - SLOW
for genome in &offspring {
    let fitness = evaluate_fitness(genome); // Blocking I/O or CPU
    population.push((genome, fitness));
}
```

## Parallel Evolution Implementation

### Step 1: Add Rayon Dependency

```toml
# Cargo.toml
[dependencies]
rayon = "1.8"
```

### Step 2: Parallel Fitness Evaluation

```rust
use rayon::prelude::*;

impl Population {
    pub fn evaluate_parallel(&mut self) {
        // Parallel fitness evaluation
        let fitnesses: Vec<f64> = self.genomes
            .par_iter()           // Parallel iterator
            .map(|genome| {
                evaluate_fitness(genome)
            })
            .collect();
        
        // Update fitness scores
        for (genome, fitness) in self.genomes.iter_mut().zip(fitnesses) {
            genome.fitness = fitness;
        }
    }
}
```

### Step 3: Thread-Safe Genome Evaluation

```rust
use std::sync::Arc;

pub struct FitnessEvaluator {
    code_wiki: Arc<CodeWiki>,
    neural_bridge: Arc<NeuralBridge>,
}

impl FitnessEvaluator {
    pub fn evaluate(&self, genome: &Genome) -> f64 {
        // Arc allows shared access across threads
        let syntax = self.code_wiki.check_syntax(&genome.code);
        let semantic = self.neural_bridge.score_semantic(&genome.changes);
        
        // Weighted combination
        0.3 * syntax + 0.4 * semantic + 0.3 * genome.quality_score
    }
}

// In population evaluation
let evaluator = Arc::new(FitnessEvaluator::new(
    Arc::clone(&code_wiki),
    Arc::clone(&neural_bridge),
));

let fitnesses: Vec<f64> = genomes
    .par_iter()
    .map(|g| evaluator.evaluate(g))
    .collect();
```

## Adaptive Mutation Rates

### Current: Fixed Rate

```rust
const MUTATION_RATE: f64 = 0.1;
```

### Proposed: Adaptive Rate

```rust
pub struct EvolutionConfig {
    base_mutation_rate: f64,
    current_rate: f64,
    generation: usize,
    fitness_history: Vec<f64>,
}

impl EvolutionConfig {
    pub fn adaptive_mutation_rate(&mut self) -> f64 {
        let avg_fitness = self.fitness_history
            .iter()
            .rev()
            .take(5)
            .sum::<f64>() / 5.0;
        
        self.current_rate = if avg_fitness > 0.9 {
            // High fitness: exploit (lower mutation)
            0.05
        } else if self.generation < 20 {
            // Early generations: explore (higher mutation)
            0.2
        } else if self.generation % 10 == 0 {
            // Periodic diversity injection
            0.25
        } else {
            // Default
            0.1
        };
        
        self.current_rate
    }
}
```

## Neural-Aware Fitness Weighting

### Current: Static Weights

```rust
const WEIGHTS: [f64; 4] = [0.25, 0.25, 0.25, 0.25];
```

### Proposed: Dynamic from Neural State

```rust
use reqwest;
use serde::Deserialize;

#[derive(Deserialize)]
struct NeuralState {
    turbulence: f64,
    attractor: String,
    dominant_node: String,
    activations: HashMap<String, f64>,
}

pub struct NeuralFitnessWeighting {
    client: reqwest::Client,
    mesh_endpoint: String,
}

impl NeuralFitnessWeighting {
    pub async fn fetch_weights(&self) -> Result<FitnessWeights, Error> {
        let state: NeuralState = self.client
            .get(&format!("{}/api/mesh/mind", self.mesh_endpoint))
            .send()
            .await?
            .json()
            .await?;
        
        // Adjust weights based on neural state
        let weights = match state.attractor.as_str() {
            "Chaos Initial" => FitnessWeights::exploratory(),
            "StableOrbit" => FitnessWeights::exploitative(),
            "StrangeAttractor" => FitnessWeights::creative(),
            _ => FitnessWeights::balanced(),
        };
        
        // Turbulence affects mutation pressure
        if state.turbulence > 0.1 {
            weights.with_mutation_boost(1.5);
        }
        
        Ok(weights)
    }
}
```

## Performance Gains

### Benchmark Results (Estimated)

| Configuration | Time per Generation | Speedup |
|--------------|---------------------|---------|
| Sequential (8 cores idle) | 8.0s | 1x |
| Parallel (8 cores) | 1.2s | **6.7x** |
| Parallel + Adaptive | 1.1s | 7.3x |
| Parallel + Neural | 1.3s | 6.2x |

*Note: Actual results depend on fitness evaluation complexity*

## Implementation Priority

### Phase 1: Parallel Evaluation (High Priority)

1. Add rayon dependency
2. Implement `par_iter()` for fitness evaluation
3. Make CodeWiki/NeuralBridge thread-safe with `Arc`
4. Benchmark before/after

### Phase 2: Adaptive Mutation (Medium Priority)

1. Track fitness history
2. Implement adaptive rate calculation
3. A/B test vs fixed rate

### Phase 3: Neural Integration (Low Priority)

1. Add mesh/mind API client
2. Implement weight adjustment logic
3. Feature flag for neural awareness

## Code Example: Full Integration

```rust
// src/evolution/population.rs
use rayon::prelude::*;
use std::sync::Arc;

pub struct Population {
    genomes: Vec<Genome>,
    config: EvolutionConfig,
    evaluator: Arc<dyn FitnessEvaluator>,
}

impl Population {
    pub fn evolve_generation(&mut self) {
        // Selection (sequential, cheap)
        let parents = self.tournament_selection();
        
        // Crossover + mutation (can be parallel)
        let offspring: Vec<Genome> = (0..self.config.pop_size)
            .into_par_iter()  // Parallel population generation
            .map(|_| {
                let parent1 = &parents[random() % parents.len()];
                let parent2 = &parents[random() % parents.len()];
                let child = Genome::crossover(parent1, parent2);
                child.mutate(self.config.adaptive_mutation_rate())
            })
            .collect();
        
        // Parallel fitness evaluation
        let fitnesses: Vec<f64> = offspring
            .par_iter()
            .map(|g| self.evaluator.evaluate(g))
            .collect();
        
        // Update population
        self.genomes = offspring.into_iter()
            .zip(fitnesses)
            .map(|(g, f)| { g.fitness = f; g })
            .collect();
        
        self.config.generation += 1;
        self.config.fitness_history.push(self.average_fitness());
    }
}
```

## Risks and Mitigations

| Risk | Mitigation |
|------|------------|
| Thread contention on CodeWiki | Use `Arc<RwLock<CodeWiki>>` with read-heavy pattern |
| Memory overhead | Limit population size; use `with_min_len()` for small workloads |
| Non-determinism | Set Rayon thread pool seed for reproducibility |
| Compilation time | Feature flag for parallel mode |

## Implementation Status

**Deferred:** Requires Rust code changes and dependency addition.

**Recommendation:** Implement Phase 1 (parallel evaluation) for immediate gains; Phase 2-3 for future optimization.
