# IronReview T430 Algorithm & OpenClaw Integration

**Based on OpenEvolve Night Cycle Report 2026-04-11**  
**Source:** IronReview v4.0 Rust-based evolutionary code reviewer

---

## Overview

IronReview is a Rust-based evolutionary code reviewer integrated with OpenClaw. It uses the **T430 Phase-Shift Algorithm**, a novel genetic approach to code evolution that mirrors neural field dynamics for intelligent code mutation and fitness evaluation.

---

## T430 Algorithm Components

### 1. Multi-Factor Fitness Function

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

### 2. Tournament Selection with Elitism

- **Selection:** Tournament selection (size 3)
- **Elitism:** Top 10% of population preserved unmodified
- **Diversity:** Maintains genetic diversity through crowding

### 3. Semantic Crossover

Line-based recombination with semantic awareness:

```rust
fn semantic_crossover(parent1: &CodeIndividual, parent2: &CodeIndividual) -> CodeIndividual {
    let crossover_point = select_semantic_boundary(parent1);
    let child_lines = [
        &parent1.lines[..crossover_point],
        &parent2.lines[crossover_point..]
    ].concat();
    
    CodeIndividual::new(child_lines)
}
```

### 4. Mutation Operators

| Operator | Description | Rate |
|----------|-------------|------|
| Variable Rename | Rename variables for clarity | 0.1 |
| Function Extraction | Extract code blocks to functions | 0.05 |
| Variable Inline | Inline simple variable usages | 0.05 |
| Import Optimization | Optimize and dedupe imports | 0.08 |
| Documentation | Add/update doc comments | 0.05 |
| Pattern Refactor | Apply known refactor patterns | 0.07 |

---

## Neural-Aware Evolution

The T430 algorithm can incorporate neural field state for fitness weighting:

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

**Rationale:** When science/engineer nodes are dominant, prioritize technical correctness. When creative node is dominant, prioritize novel solutions.

---

## CodeWiki Integration

IronReview connects to CodeWiki MCP server for pattern retrieval:

```rust
pub struct CodeWikiClient {
    mcp_client: McpClient,
    cache: HashMap<String, CodePattern>,
}

impl CodeWikiClient {
    pub async fn get_pattern(&mut self, query: &str) -> Result<CodePattern> {
        // Check cache first
        if let Some(cached) = self.cache.get(query) {
            return Ok(cached.clone());
        }
        
        // Query MCP server
        let pattern = self.mcp_client.call("codewiki/getPattern", query).await?;
        self.cache.insert(query.to_string(), pattern.clone());
        
        Ok(pattern)
    }
}
```

### Fallback Strategy

**Issue:** No fallback if codewiki-mcp binary unavailable.

**Solution:** Implement multi-tier fallback:

```rust
pub enum CodeWikiMode {
    Direct,      // MCP server (preferred)
    Cached,      // Local pattern cache
    Fallback,    // Static rule-based patterns
}

impl CodeWikiClient {
    pub async fn with_fallback() -> Result<Self> {
        match Self::new_mcp().await {
            Ok(client) => Ok(client),
            Err(_) => {
                warn!("MCP unavailable, using fallback mode");
                Self::new_cached()
            }
        }
    }
}
```

---

## Secure Command Execution

IronReview uses `SecureCommand` wrapper for safe execution:

```rust
pub struct SecureCommand {
    whitelist: HashSet<String>,
    timeout: Duration,
}

impl SecureCommand {
    pub fn new() -> Self {
        let whitelist = HashSet::from([
            "cargo".to_string(),
            "rustc".to_string(),
            "git".to_string(),
        ]);
        
        Self {
            whitelist,
            timeout: Duration::from_secs(60),
        }
    }
    
    pub async fn run(&self, cmd: &str) -> Result<Output> {
        if !self.whitelist.contains(cmd) {
            return Err(Error::CommandNotAllowed(cmd.to_string()));
        }
        
        tokio::time::timeout(self.timeout, process::Command::new(cmd).output()).await?
    }
}
```

---

## Async Parallel Evolution

**Current:** Sequential population evolution  
**Recommended:** Parallel fitness evaluation with `rayon`:

```rust
use rayon::prelude::*;

impl EvolutionEngine {
    pub fn next_generation_parallel(&self, population: &[CodeIndividual]) -> Result<Vec<CodeIndividual>> {
        let offspring_count = self.config.population_size - self.elite_count;
        
        let offspring: Vec<CodeIndividual> = (0..offspring_count)
            .into_par_iter()
            .map(|_| {
                let parent1 = self.select_parent(population)?;
                let parent2 = self.select_parent(population)?;
                self.crossover_and_mutate(parent1, parent2)
            })
            .collect::<Result<Vec<_>>>()?;
        
        let mut new_population = self.select_elites(population);
        new_population.extend(offspring);
        
        Ok(new_population)
    }
}
```

---

## Adaptive Mutation Rates

**Current:** Fixed 0.1 mutation rate  
**Recommended:** Dynamic adaptation:

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

---

## OpenClaw Session Store Integration

Persist populations across sessions:

```rust
use openclaw::library::{loadSessionStore, saveSessionStore};

pub struct PersistentEvolution {
    session_key: String,
}

impl PersistentEvolution {
    pub async fn save_population(&self, population: &[CodeIndividual]) -> Result<()> {
        let serialized = serde_json::to_string(population)?;
        save_session_store(&self.session_key, serialized).await
    }
    
    pub async fn load_population(&self) -> Result<Vec<CodeIndividual>> {
        let serialized = load_session_store(&self.session_key).await?;
        let population: Vec<CodeIndividual> = serde_json::from_str(&serialized)?;
        Ok(population)
    }
}
```

---

## Improvement Priorities

### High Priority
1. **CodeWiki MCP Health Monitoring** - Add fallback mode
2. **Async Parallel Evolution** - Use rayon for concurrent evaluation
3. **Neural-Aware Fitness** - Weight by neural activation

### Medium Priority
4. **Adaptive Mutation Rates** - Dynamic rate based on convergence
5. **Population Persistence** - Session store integration

### Low Priority
6. **AST-Aware Mutations** - Structural refactoring instead of line-based
7. **Incremental Compilation Cache** - Speed up fitness evaluation

---

## References

- IronReview Repository: `/mnt/nvme_secondary/ai_projects/openclaw/IronReview/`
- T430 Algorithm Paper: (Internal documentation)
- MCP Protocol: https://modelcontextprotocol.io

---

*Generated from OpenEvolve Night Cycle Report*
*Date: 2026-04-11*
