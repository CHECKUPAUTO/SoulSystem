# Evolution Engine: Architectural Blueprint

## 1. The Convergence Matrix — Darwin Gödel Machine × Regularized Evolution × Reflexion

How three paradigm-shifting papers fuse into one runtime.

### Darwin Gödel Machine (2025) + Regularized Evolution (2018)

```
DarwinGM supplies:   | Regularized Evolution supplies:
  Open-ended archive |   Tournament sampling
  Novelty weighting  |   Age-based regularization
  Quality selection  |   Random mutation
  ↓                  |   ↓
  Converge into: AgentArchive with fitness-proportional + novelty-weighted
  sampling, capped at 100 entries, prune when full (remove lowest quality × age).
```

The key insight: **Fitness functions are replaced by runtime telemetry.**  
In standard evolution, you need an explicit fitness function. Here, `quality_score` (from `audit.rs` health checks) + `novelty_score` (1 − max cosine similarity to archive) serve as the dual objective. The telemetry is:

```
fitness(agent) = quality(agent) × novelty(agent)
  where quality = frontmatter completeness + tool coverage + existing score
  where novelty = how different this agent is from all archive entries
```

### Reflexion Integration — The Feedback Signal

Reflexion bridges the gap between raw telemetry and actionable direction:

```
◄── DarwinGM selects candidate agents ──►
    │                                       │
    └── Reflexion evaluates outcome ────────┘
         │
         └── success_patterns[] → reinforce similar mutations
         └── failure_patterns[] → avoid similar mutations
```

**Convergence mechanism:**
1. Darwin Archive samples 5 candidates via tournament selection
2. Each candidate is mutated (description change, model change, tool addition)
3. The mutated agent is validated → if pass, added to archive
4. Reflexion records the cycle outcome (did health improve? which mutation worked?)
5. Reflexion's pattern lists guide the *next* Darwin selection weights

```
Concrete example:
  Archive samples agent-a (quality=0.92, novelty=0.3)
  Mutation: add "golang-reviewer" language
  Validation: pass
  Effect: health 82.7% → 83.1%
  Reflexion records: "mutation: add language → success"
  Next iteration weights: language-add mutations weighted ×1.2
```

---

## 2. State-Space & Memory Mechanics

Dual-tier memory architecture, inspired by Gödel Agent (Schmidhuber 2007, Paper 12) and Voyager (Wang et al. 2023, Paper 25).

### Tier 1: Ephemeral Working Memory (Gödel Agent style)

Lives in `EvolState` — the runtime state of the current evolution cycle:

| Field | Type | Purpose |
|-------|------|---------|
| `phase` | `u8` | Current phase (0-10) |
| `health_history` | `Vec<f64>` | Last N health scores for derivative calc |
| `current_strategy` | `EvolutionStrategy` | Active strategy this cycle |
| `applied_count` | `u32` | How many proposals applied this cycle |
| `self_mod_proposals` | `Vec<SelfModProposal>` | Pending Gödel self-mods |
| `active_reflections` | `Vec<String>` | Reflexion outputs for current cycle |

This memory is **not persisted** — it resets each run. It mirrors Gödel Agent's "current state" that feeds into the self-modification utility function.

### Tier 2: Vectorized Skill/Pattern Archive (Voyager style)

Persists across runs in `AgentArchive` + `ReflexionMemory`:

```
AgentArchive
  ├── entries[100]: ArchiveEntry { def, quality, novelty, parent_id, generation, tags }
  └── tag_distribution: HashMap<String, usize>
  
ReflexionMemory
  ├── episodes[100]: ReflexionEpisode { action, outcome, reflection, lessons, state }
  ├── success_patterns: Vec<String>
  └── failure_patterns: Vec<String>
```

**Voyager-style "skill library":**
- Each `ArchiveEntry.def` is a complete agent definition (YAML frontmatter + body)
- `ArchiveEntry.tags` serve as skill descriptors ("reviewer", "golang", "code-gen")
- When a gap is detected (`analyze` finds missing language), the archive is queried for the most similar skill, which gets mutated to fill the gap

### Memory Flow Diagram

```
                    ┌──────────────────────┐
                    │  Ephemeral (EvolState) │  ← Dies after each cycle
                    │  phase, health, mods   │
                    └────┬───────────────────┘
                         │ write important patterns
                         ▼
                    ┌──────────────────────┐
                    │  Persistent Archive   │  ← Survives across runs
                    │  Agents + Reflections │
                    └────┬───────────────────┘
                         │ sample + mutate
                         ▼
                    ┌──────────────────────┐
                    │  Next Cycle: EvolState │
                    └────────────────────────┘
```

---

## 3. The Open-Ended Exploration Engine

From "Why Greatness Cannot Be Planned" (Stanley & Lehman 2015, Paper 29): **objective-based search converges to local optima; novelty-based search discovers stepping stones.**

### The Problem with Naive Evolution

```
Objective: "Maximize ecosystem health"
  → All mutations focus on health-score optimization
  → Archive fills with similar high-quality agents
  → Diversity collapses → no stepping stones to breakthrough
  → Health plateau at ~85%
```

### The Solution: Mutation Taxonomy

Instead of one mutation type, we maintain a **diversity of mutation strategies**, each exploring a different axis:

| Strategy | Axis | Mechanism | Escape local optimum? |
|----------|------|-----------|----------------------|
| `GenerateMissingLanguage` | Coverage | Creates agent for new lang | Adds entirely new capability |
| `ImproveLowQuality` | Quality | Fixes incomplete agents | Incremental improvement |
| `ArchiveDiversification` | Novelty | Samples least-similar pair | Forces diversity |
| `FixIncomplete` | Completeness | Adds missing fields | Quick wins |
| `RandomArchive` | Exploration | 10% random mutation rate | Prevents convergence |
| `SelfPlayCompetition` | Performance | Champion vs challenger | Pressure to improve |

### Stepping Stone Detection

The `ExplosionMetrics` system doubles as a **stepping stone detector**:

```
If health increases AND novelty increases:
  → This mutation discovered a stepping stone
  → Weigh this mutation type higher in future

If health increases but novelty decreases:
  → This mutation is exploitative (refining existing)
  → Normal weight

If health decreases:
  → Record as failure_pattern (Reflexion)
  → Reduce weight of this mutation type
```

### Concrete Novelty Computation

```
novelty_score(new_agent, archive) = 1 − max(cosine_similarity(new_agent, existing))
  where similarity is computed on:
    - Language set overlap (rust = {rust}, go = {go})
    - Tool set overlap (if same tools → higher similarity)
    - Model match (both "opus" → higher similarity)
    - Description embedding approximation (word overlap Jaccard)
```

### Avoiding Local Minima — The 10% Rule

At every generation:

1. **90% of mutations** are directed by the best strategy (exploitation)
2. **10% of mutations** are random (exploration)
3. If the best strategy hasn't improved in 3 cycles, exploration jumps to 30%

This is implemented in the Improver's plateau detection:

```
if performance_history.last(3).all(|&v| v <= plateau_threshold):
    Improver::mutate_strategy()
    → changes Strategy from e.g. GeneticAlgorithm → RandomMutation
    → exploration rate 10% → 30%
```

---

## Putting It All Together: The Meta-Evolution Loop

```
┌─────────────────────────────────────────────────────────────┐
│                     META-EVOLUTION LOOP                      │
│                                                             │
│  ┌─────────┐    ┌──────────┐    ┌───────────┐              │
│  │  AUDIT  │───→│  ANALYZE │───→│  GENERATE │              │
│  │ (health)│    │ (gaps)   │    │ (proposals)│              │
│  └─────────┘    └──────────┘    └─────┬─────┘              │
│                                       │                     │
│                                       ▼                     │
│  ┌─────────┐    ┌──────────┐    ┌───────────┐              │
│  │  APPLY  │←───│ VALIDATE │←───│ (proposals)│              │
│  │ (write) │    │ (check)  │    │           │              │
│  └────┬────┘    └──────────┘    └───────────┘              │
│       │                                                    │
│       ▼                                                    │
│  ┌─────────────────────────────────────────────────────┐   │
│  │              META-EVOLUTION (7 Pillars)              │   │
│  │                                                      │   │
│  │  1. Gödel Engine: evaluate_strategies()              │   │
│  │  2. Darwin Archive: add_high_quality()               │   │
│  │  3. STOP Improver: self_improve()                    │   │
│  │  4. Self-Play: compete(champion, challenger)         │   │
│  │  5. Reflexion: record(outcome)                       │   │
│  │  6. OPRO Trajectory: record(score, strategy)         │   │
│  │  7. ExplosionMetrics: compute_derivatives()          │   │
│  │                                                      │   │
│  │  ┌─ If explosion → add safety constraints ──────┐   │   │
│  │  └─ Gödel self-mod → strategy mutation → loop ──┘   │   │
│  └─────────────────────────────────────────────────────┘   │
│       │                                                    │
│       └─────→ REPEAT (recursive auto-call) ────────────────┘
└─────────────────────────────────────────────────────────────┘
```

### State-Space Projection

The system occupies a projection of `6` continuous dimensions + `N` categorical:

| Dimension | Range | Current | Goal |
|-----------|-------|---------|------|
| `health` | [0, 1] | 0.827 | → 0.95+ |
| `diversity` | [0, 1] | ~0.4 | → 0.7+ |
| `coverage` | [0, 1] | 27/50 langs | → 45/50 |
| `acceleration (d²)` | (−∞, ∞) | ~0.002 | → sustained >0 |
| `archive_size` | [0, 100] | 0 (seed) | → 80+ |
| `strategy_effectiveness` | [0, 1] | 0.5 (initial) | → 0.9+ |

The exploration engine ensures we don't greedily climb health → coverage → health, but instead discover stepping stones in all dimensions simultaneously.

---

## Operational Workflow

```bash
# 1. Full meta-evolution cycle (standard)
aevolve recursive --max-iter 5 --apply

# 2. Check if we're in an explosion
aevolve status  # shows health_history + derivatives

# 3. Inspect the archive (TODO: add --show-archive flag)
# aevolve status --verbose

# 4. Run a single cycle to manually inspect
aevolve evolve --dry-run   # shows proposals without writing
aevolve evolve --apply     # writes proposals

# 5. Full report
aevolve full-report
```
