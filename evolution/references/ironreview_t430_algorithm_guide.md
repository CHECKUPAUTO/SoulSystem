# IronReview T430 Phase-Shift Algorithm Guide

**Source:** OpenEvolve Night Cycle Report 2026-04-12 01:15 UTC  
**Classification:** Comprehensive Evolutionary Analysis  
**Model Consensus:** gemma4:31b-cloud + kimi-k2.5:cloud

---

## Overview

The T430 Phase-Shift Algorithm is IronReview's multi-factor fitness evaluation system for identifying evolutionary improvements in codebases. It employs adaptive mutation rates, structured pattern recognition, and cross-repository analysis.

---

## Multi-Factor Fitness Function

```rust
pub struct CodeFitness {
    pub syntax_score: f64,      // 30% - AST validity
    pub semantic_score: f64,    // 40% - Relevance to goal
    pub quality_score: f64,     // 20% - Code style, complexity
    pub security_score: f64,    // 10% - Security audit
}

impl FitnessFunction for CodeFitness {
    fn evaluate(&self, individual: &CodeIndividual) -> f64 {
        self.syntax_score * 0.30 +
        self.semantic_score * 0.40 +
        self.quality_score * 0.20 +
        self.security_score * 0.10
    }
}
```

### Scoring Breakdown

| Factor | Weight | Evaluation Criteria |
|--------|--------|---------------------|
| Syntax | 30% | AST validity, compilation success, type correctness |
| Semantic | 40% | Relevance to identified goal, pattern correctness |
| Quality | 20% | Code style adherence, complexity metrics, documentation |
| Security | 10% | Vulnerability scan, SSRF, injection risks, secrets handling |

---

## Mutation Operators

| Operator | Description | Rate |
|----------|-------------|------|
| Variable Rename | Rename variables for clarity | 0.10 |
| Function Extraction | Extract code blocks to functions | 0.05 |
| Import Optimization | Optimize and dedupe imports | 0.08 |
| Documentation | Add/update doc comments | 0.05 |
| Pattern Refactor | Apply known refactor patterns | 0.07 |

---

## Adaptive Mutation Rates

```rust
fn adaptive_mutation_rate(generation: usize, avg_fitness: f64) -> f64 {
    if avg_fitness > 0.9 {
        0.05  // Exploit good solutions (convergence)
    } else if generation < 20 {
        0.2   // Explore early (diversity)
    } else if generation % 10 == 0 {
        0.25  // Periodic diversity injection
    } else {
        0.1   // Default
    }
}
```

### Rate Strategy

- **High Fitness (>0.9):** Reduce mutation to exploit convergence
- **Early Generations (<20):** High mutation for exploration
- **Periodic Injection:** Every 10th generation, spike diversity
- **Default:** Balanced exploration/exploitation

---

## Phase-Shift Detection

The algorithm detects when evolutionary search should shift phases:

```rust
enum EvolutionPhase {
    Exploration,      // High diversity, seeking promising regions
    Exploitation,     // Converging on good solutions
    Convergence,      // Fine-tuning near-optimal solutions
    Divergence,       // Stuck in local optima, inject diversity
}

fn detect_phase(population: &Population) -> EvolutionPhase {
    let diversity = population.genetic_diversity();
    let avg_fitness = population.average_fitness();
    let improvement_rate = population.improvement_rate_last_n(10);
    
    match (diversity, avg_fitness, improvement_rate) {
        (d, _, _) if d > 0.8 => EvolutionPhase::Exploration,
        (d, f, _) if d < 0.3 && f > 0.85 => EvolutionPhase::Convergence,
        (_, _, r) if r < 0.01 => EvolutionPhase::Divergence,
        _ => EvolutionPhase::Exploitation,
    }
}
```

---

## Repository Analysis Patterns

### Commit Analysis

```rust
struct CommitAnalysis {
    hash: String,
    message: String,
    files_changed: Vec<PathBuf>,
    fitness: CodeFitness,
    pattern_detected: Option<Pattern>,
}

fn analyze_commits(commits: Vec<Commit>) -> Vec<CommitAnalysis> {
    commits.iter()
        .map(|c| CommitAnalysis {
            hash: c.hash.clone(),
            message: c.message.clone(),
            files_changed: c.files_changed.clone(),
            fitness: evaluate_fitness(c),
            pattern_detected: detect_pattern(c),
        })
        .collect()
}
```

### Pattern Recognition

| Pattern | Detection | Action |
|---------|-----------|--------|
| OAuth Scope Handling | `scopes` in diff | Flag for security review |
| Pure Test Migration | `test:` + `pure` in message | Document pattern |
| Performance Optimization | `perf:` prefix | Benchmark before/after |
| Auto-Generated Proposals | `SoulLink` or `proposals/` | Formalize as template |

---

## Cross-Repository Integration

### Integration Scoring

```rust
struct CrossRepoOpportunity {
    source_repo: String,
    target_repo: String,
    integration_type: IntegrationType,
    effort_estimate: EffortLevel,
    value_estimate: ValueLevel,
}

enum IntegrationType {
    SkillIntegration,      // OpenClaw skill for external service
    ApiBridge,             // API wrapper/connector
    DataPipeline,          // Data flow between systems
    UiExtension,           // Visual/UI integration
}
```

---

## Neural Metrics Integration

T430 can weight evolution by neural activation:

```rust
pub struct NeuralFitness {
    pub science_weight: f64,
    pub engineer_weight: f64,
    pub creative_weight: f64,
}

impl FitnessFunction for NeuralFitness {
    fn evaluate(&self, individual: &CodeIndividual) -> f64 {
        let base_score = self.base_evaluation(individual);
        // Weight by current neural activation
        base_score * (self.science_weight + self.engineer_weight)
    }
}
```

**Rationale:** When science/engineer nodes dominate, prioritize technical correctness. When creative node dominates, prioritize novel solutions.

---

## Model Consensus System

| Finding | gemma4:31b-cloud | kimi-k2.5:cloud | Consensus |
|---------|------------------|-----------------|-----------|
| Critical Issue | ✅ | ✅ | Strong |
| High Priority | ✅ | ✅ | Strong |
| Medium Priority | ✅ | ⚠️ | Medium |
| Novel Pattern | ✅ | ✅ | Strong |

---

## Implementation Recommendations

### 1. Parallel Fitness Evaluation

```rust
use rayon::prelude::*;

fn parallel_evaluate(population: &mut Population) {
    population.individuals
        .par_iter_mut()
        .for_each(|individual| {
            individual.fitness = evaluate_fitness(individual);
        });
}
```

### 2. Checkpointing

```rust
fn save_checkpoint(generation: usize, population: &Population) {
    let checkpoint = Checkpoint {
        generation,
        population: population.clone(),
        timestamp: Utc::now(),
    };
    
    std::fs::write(
        format!("checkpoints/gen_{}.json", generation),
        serde_json::to_string(&checkpoint).unwrap()
    ).unwrap();
}
```

### 3. Early Stopping

```rust
fn should_stop(generation: usize, 
               best_fitness: f64,
               history: &Vec<f64>) -> bool {
    // Stop if converged
    if generation > 100 && 
       history.iter().rev().take(20).all(|&f| f == best_fitness) {
        return true;
    }
    
    // Stop if fitness target reached
    if best_fitness > 0.95 {
        return true;
    }
    
    false
}
```

---

## References

- Night Cycle Report: night_cycle_20260412_0115.md
- IronReview v4.0 Documentation
- T430 Phase-Shift Algorithm Specification