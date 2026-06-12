# Neural-Aware Fitness Weighting

**Source:** OpenEvolve Night Cycle Report 2026-04-12 05:02  
**Author:** IronReview T430 Algorithm (Mutation 4)  
**Priority:** P2 - Medium Priority  
**Classification:** Evolutionary Algorithm Pattern / Machine Learning

---

## Problem Statement

**Static Fitness Weights Limit Adaptation:** IronReview's T430 algorithm uses fixed fitness weights:
- Syntax: 30%
- Semantic: 40%
- Quality: 20%
- Security: 10%

**Problems:**
- Cannot adapt to different code review contexts
- Science-heavy reviews need technical correctness prioritization
- Creative phases need novelty prioritization
- Static weights don't reflect current system state

---

## Solution: Dynamic Fitness Weighting Based on Neural State

### Core Concept

Map neural field activations to fitness weights:

```
Neural State → Fitness Weights

Science Activation → Technical correctness weight
Engineer Activation → Performance/optimization weight  
Creative Activation → Novelty/code quality weight
```

### Implementation

#### 1. Neural Fitness Structure

```rust
// IronReview/src/evolution/neural_fitness.rs

use crate::neural::NeuralState;

/// Neural-aware fitness configuration
#[derive(Debug, Clone)]
pub struct NeuralFitness {
    /// Weight for technical correctness (syntax, security)
    pub science_weight: f64,
    /// Weight for performance and practical utility
    pub engineer_weight: f64,
    /// Weight for novel solutions and quality
    pub creative_weight: f64,
}

impl NeuralFitness {
    /// Create from current neural field state
    pub fn from_neural_state(state: &NeuralState) -> Self {
        let total = state.science + state.engineer + state.creative;
        
        Self {
            science_weight: state.science / total,
            engineer_weight: state.engineer / total,
            creative_weight: state.creative / total,
        }
    }
    
    /// Create with explicit weights (for testing)
    pub fn with_weights(science: f64, engineer: f64, creative: f64) -> Self {
        let total = science + engineer + creative;
        
        Self {
            science_weight: science / total,
            engineer_weight: engineer / total,
            creative_weight: creative / total,
        }
    }
}

impl Default for NeuralFitness {
    fn default() -> Self {
        // Balanced weights when no neural state available
        Self {
            science_weight: 0.4,
            engineer_weight: 0.35,
            creative_weight: 0.25,
        }
    }
}
```

#### 2. Neural-Aware Fitness Calculation

```rust
// IronReview/src/evolution/fitness.rs

use crate::neural_fitness::NeuralFitness;
use crate::individual::CodeIndividual;

/// Calculate fitness with neural-weighted components
pub fn calculate_neural_fitness(
    individual: &CodeIndividual,
    neural_weights: &NeuralFitness,
) -> Result<f64, FitnessError> {
    // Component scores (0.0 - 1.0)
    let syntax_score = check_syntax(&individual.source_code)?;
    let semantic_score = semantic_similarity(
        &individual.source_code,
        &individual.target_description
    )?;
    let quality_score = code_quality(&individual.source_code)?;
    let security_score = security_audit(&individual.source_code)?;
    
    // Combine technical scores (science-weighted)
    let technical = syntax_score * 0.5 + security_score * 0.5;
    
    // Combine practical scores (engineer-weighted)
    let practical = semantic_score * 0.7 + quality_score * 0.3;
    
    // Quality is creative domain
    let novelty = quality_score;
    
    // Apply neural weights
    let fitness = (
        technical * neural_weights.science_weight +
        practical * neural_weights.engineer_weight +
        novelty * neural_weights.creative_weight
    ).min(1.0);
    
    Ok(fitness)
}

/// Legacy static fitness (for comparison)
pub fn calculate_static_fitness(individual: &CodeIndividual) -> Result<f64, FitnessError> {
    let syntax_score = check_syntax(&individual.source_code)?;
    let semantic_score = semantic_similarity(
        &individual.source_code,
        &individual.target_description
    )?;
    let quality_score = code_quality(&individual.source_code)?;
    let security_score = security_audit(&individual.source_code)?;
    
    // Static weights
    let fitness = (
        syntax_score * 0.30 +
        semantic_score * 0.40 +
        quality_score * 0.20 +
        security_score * 0.10
    ).min(1.0);
    
    Ok(fitness)
}
```

#### 3. Integration with Evolution Engine

```rust
// IronReview/src/evolution/engine.rs

use crate::neural::get_current_neural_state;
use crate::neural_fitness::NeuralFitness;

pub struct EvolutionEngine {
    config: EvolutionConfig,
    neural_fitness: NeuralFitness,
}

impl EvolutionEngine {
    pub fn new(config: EvolutionConfig) -> Self {
        // Initialize with current neural state
        let neural_state = get_current_neural_state();
        let neural_fitness = NeuralFitness::from_neural_state(&neural_state);
        
        println!(
            "EvolutionEngine initialized with neural weights: {:?}",
            neural_fitness
        );
        
        Self {
            config,
            neural_fitness,
        }
    }
    
    /// Evaluate population with neural-aware fitness
    pub fn evaluate_population(
        &self,
        population: &[CodeIndividual]
    ) -> Result<Vec<EvaluatedIndividual>, EvolutionError> {
        population
            .iter()
            .map(|individual| {
                let fitness = calculate_neural_fitness(
                    individual,
                    &self.neural_fitness
                )?;
                
                Ok(EvaluatedIndividual {
                    individual: individual.clone(),
                    fitness,
                })
            })
            .collect()
    }
    
    /// Update neural weights (call periodically or on neural state change)
    pub fn update_neural_weights(&mut self) {
        let neural_state = get_current_neural_state();
        self.neural_fitness = NeuralFitness::from_neural_state(&neural_state);
        
        println!(
            "Updated neural fitness weights: science={:.2}, engineer={:.2}, creative={:.2}",
            self.neural_fitness.science_weight,
            self.neural_fitness.engineer_weight,
            self.neural_fitness.creative_weight
        );
    }
}

/// Generation context with neural state
#[derive(Debug)]
pub struct GenerationContext {
    pub generation: usize,
    pub neural_state: NeuralState,
    pub fitness_weights: NeuralFitness,
}

impl GenerationContext {
    pub fn capture() -> Self {
        let neural_state = get_current_neural_state();
        let fitness_weights = NeuralFitness::from_neural_state(&neural_state);
        
        Self {
            generation: 0,  // Set by engine
            neural_state,
            fitness_weights,
        }
    }
}
```

---

## Neural State Mappings

### Current State: Chaos Initial

```yaml
# Turbulence: 0.0939 (stable)
# Dominant: science (38.6%)
# Secondary: engineer (34.7%)
# Creative: (29.5%)

neural_state:
  turbulence: 0.0939
  attractor: "Chaos Initial"
  
  activations:
    science: 0.386
    engineer: 0.347
    creative: 0.295

fitness_weights:
  science: 0.386    # Technical correctness priority
  engineer: 0.347  # Performance priority
  creative: 0.295  # Novelty priority
```

### Example Mappings

| Neural State | Science | Engineer | Creative | Interpretation |
|--------------|---------|----------|----------|----------------|
| **Deep Basin** | 0.25 | 0.25 | 0.50 | Creative mode - prioritize novel solutions |
| **Stable Orbit** | 0.40 | 0.40 | 0.20 | Standard mode - balanced technical focus |
| **Chaos Initial** | 0.39 | 0.35 | 0.30 | Analytical mode - correctness matters most |
| **Transient** | 0.33 | 0.33 | 0.33 | Transition mode - equal weights |

---

## Application Contexts

### Security Reviews (High Science)

```rust
// When turbulence > 0.1 (excited state)
// Prioritize security and syntax correctness

if neural_state.turbulence > 0.1 {
    // Boost science weight for security reviews
    let security_weights = NeuralFitness::with_weights(
        0.50,  // science
        0.30,  // engineer
        0.20,  // creative
    );
}
```

### Performance Optimization (High Engineer)

```rust
// When engineer node is dominant
// Prioritize performance and efficiency

if neural_state.engineer > 0.40 {
    let performance_weights = NeuralFitness::with_weights(
        0.25,  // science
        0.50,  // engineer
        0.25,  // creative
    );
}
```

### Experimental Features (High Creative)

```rust
// When creative node is dominant
// Prioritize novel solutions

if neural_state.creative > 0.40 {
    let experimental_weights = NeuralFitness::with_weights(
        0.20,  // science
        0.30,  // engineer
        0.50,  // creative
    );
}
```

---

## Configuration

```yaml
# ironreview.yaml
evolution:
  neural_fitness:
    enabled: true
    update_interval_generations: 5
    
    # Weight multipliers for different modes
    modes:
      security_review:
        science_multiplier: 1.3
        engineer_multiplier: 0.9
        creative_multiplier: 0.8
        
      performance_review:
        science_multiplier: 0.9
        engineer_multiplier: 1.3
        creative_multiplier: 0.8
        
      experimental:
        science_multiplier: 0.8
        engineer_multiplier: 0.9
        creative_multiplier: 1.3
        
      standard:
        science_multiplier: 1.0
        engineer_multiplier: 1.0
        creative_multiplier: 1.0
```

---

## Testing

```rust
// IronReview/tests/neural_fitness_test.rs

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_neural_fitness_from_state() {
        let state = NeuralState {
            science: 0.40,
            engineer: 0.35,
            creative: 0.25,
            turbulence: 0.05,
            attractor: Attractor::DeepBasin,
        };
        
        let fitness = NeuralFitness::from_neural_state(&state);
        
        // Weights should sum to 1.0
        let total = fitness.science_weight 
            + fitness.engineer_weight 
            + fitness.creative_weight;
        assert!((total - 1.0).abs() < 0.001);
        
        // Science should have highest weight
        assert!(fitness.science_weight > fitness.engineer_weight);
        assert!(fitness.science_weight > fitness.creative_weight);
    }

    #[test]
    fn test_neural_fitness_calculation() {
        let neural_weights = NeuralFitness::with_weights(0.4, 0.4, 0.2);
        
        // Mock individual with perfect scores
        let individual = CodeIndividual::mock()
            .with_syntax_score(1.0)
            .with_semantic_score(1.0)
            .with_quality_score(1.0)
            .with_security_score(1.0);
        
        let fitness = calculate_neural_fitness(&individual, &neural_weights)
            .unwrap();
        
        // Perfect scores should give ~1.0 fitness
        assert!((fitness - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_science_dominant_prioritizes_technical() {
        let state = NeuralState {
            science: 0.60,
            engineer: 0.20,
            creative: 0.20,
            turbulence: 0.05,
            attractor: Attractor::ChaosInitial,
        };
        
        let weights = NeuralFitness::from_neural_state(&state);
        
        // Create individuals with different trade-offs
        let technical_individual = CodeIndividual::mock()
            .with_syntax_score(1.0)      // Perfect syntax
            .with_semantic_score(0.5)     // Poor semantic
            .with_quality_score(0.5)
            .with_security_score(1.0);    // Perfect security
            
        let creative_individual = CodeIndividual::mock()
            .with_syntax_score(0.5)        // Poor syntax
            .with_semantic_score(1.0)       // Perfect semantic
            .with_quality_score(1.0)
            .with_security_score(0.5);
        
        let technical_fitness = calculate_neural_fitness(
            &technical_individual,
            &weights
        ).unwrap();
        
        let creative_fitness = calculate_neural_fitness(
            &creative_individual,
            &weights
        ).unwrap();
        
        // Technical should win when science dominates
        assert!(technical_fitness > creative_fitness);
    }
}
```

---

## Comparison: Static vs Neural-Aware

| Metric | Static Weights | Neural-Aware |
|--------|---------------|--------------|
| Adaptability | None | Real-time |
| Context Sensitivity | Low | High |
| Performance Reviews | Suboptimal | Optimized |
| Security Reviews | Suboptimal | Optimized |
| Experimental Code | Suboptimal | Optimized |

### Example Evolution Trace

```
Generation 1-10 (Stable Orbit):
  Weights: 0.40/0.35/0.25
  Focus: Balanced improvements
  
Generation 11-20 (High Turbulence):
  Weights: 0.50/0.30/0.20
  Focus: Conservative, correctness-focused
  
Generation 21-30 (Deep Basin):
  Weights: 0.25/0.25/0.50
  Focus: Novel solutions, exploration
```

---

## Integration with T430 Algorithm

```rust
// IronReview/src/t430/mod.rs

/// T430 Phase-Shift with Neural Integration
pub struct T430NeuralPhaseShift {
    engine: EvolutionEngine,
    neural_adapter: NeuralAdapter,
}

impl T430NeuralPhaseShift {
    pub fn run_generation(&mut self, population: &[CodeIndividual]) 
        -> Result<GenerationResult, T430Error> {
        
        // Update neural weights based on current state
        self.engine.update_neural_weights();
        
        // Evaluate with neural-aware fitness
        let evaluated = self.engine.evaluate_population(population)?;
        
        // Calculate population fitness statistics
        let avg_fitness = evaluated.iter()
            .map(|e| e.fitness)
            .sum::<f64>() / evaluated.len() as f64;
        
        // Adapt mutation rate based on neural state
        let mutation_rate = self.adapt_mutation_rate(avg_fitness);
        
        // Generate next generation with semantic crossover
        let offspring = self.semantic_crossover(&evaluated, mutation_rate)?;
        
        Ok(GenerationResult {
            population: offspring,
            avg_fitness,
            mutation_rate,
            neural_state: get_current_neural_state(),
        })
    }
    
    fn adapt_mutation_rate(&self, avg_fitness: f64) -> f64 {
        let state = get_current_neural_state();
        
        // High convergence + low turbulence = reduce mutations
        if avg_fitness > 0.9 && state.turbulence < 0.1 {
            0.05  // Exploit
        }
        // Low generation + high creative = increase mutations
        else if self.engine.generation < 20 && state.creative > 0.4 {
            0.20  // Explore
        }
        // Periodic diversity injection
        else if self.engine.generation % 10 == 0 {
            0.25
        }
        else {
            0.10  // Default
        }
    }
}
```

---

## Related Patterns

- **IronReview T430 Integration**: `ironreview_t430_integration.md`
- **Adaptive Mutation Rates**: `adaptive_mutation_rates_guide.md`
- **Circuit Breaker Pattern**: `circuit_breaker_pattern.md`

---

## References

- Night Cycle Report: `night_cycle_20260412_0502.md`
- Neural State: Turbulence 0.0939 | Chaos Initial | Science 38.6%
- T430 Mutation 4: Neural-Aware Fitness Weighting

---

*Generated by OpenEvolve Auto-Apply*  
*Classification: P2 Medium Priority ML Pattern*  
*Credit: IronReview T430 Neural Integration*
