# Brain Organ Evolution Roadmap

> Consolidated from 12 night cycle reports: 2026-04-13 11:13 through 2026-04-14 14:02

## Current Architecture

| Organ | Port | Status | Purpose | Neurons | Attractor | Pressure | Activation |
|:---|:---|:---|:---|:---|:---|:---|:---|
| Science | 9010 | ✅ Active v6.1 | Scientific analysis, verification | 400 | DeepBasin | 0.362 | 0.386 |
| Mind | 9011 | ✅ Active v6.1 | Core cognition, general intelligence | 400 | DeepBasin | 0.443 | 0.301 |
| Engineer | 9012 | ✅ Active v6.1 | Technical implementation, coding | 400 | DeepBasin | 0.272 | 0.347 |
| Crypto | 9013 | ✅ Active v6.1 | Cryptographic operations | 400 | DeepBasin | 0.348 | 0.271 |
| Creative | 9014 | ✅ Active v6.1 | Creative generation | 400 | DeepBasin | 0.322 | 0.295 |
| Meta | 9015 | ✅ Active v6.1 | Meta-cognition, self-reflection | 400 | DeepBasin | 0.463 | 0.266 |

**Orchestrator**: `soullink-orchestrator` (Rust v3.0.0) — handles routing, metrics, and cross-node coordination.

**Current Health** (2026-04-14 14:02 cycle, Cycle 131):
- Turbulence: 0.094 (near 0.1 threshold, stable-low)
- Attractor: Chaos Initial (att_000)
- Dominant node: science (38.6%)
- All 6 nodes: online, DeepBasin, N=400 each, v6.1
- Total neurons: 2400
- Mesh health: 100%
- 8 organ services running (orchestrator + 7 organs: memory/reflex/affect/perception/reasoning/language/integration)
- Orchestrator: 1d14h uptime, regulation "excited"
- System resources: 159G/915G disk (19%), 20G/125G RAM (16%) — large headroom
- Brain data: ~198M total (6×33M, ~2374 files/node)
- ⚠️ `nightly-insights` cron failing (kimi-k2.5 model likely unavailable)

**Key finding (02:30)**: The mesh is *structurally* healthy but *functionally* dormant. All nodes sit in DeepBasin with zero turbulence. The brain exists but nothing is stimulating it — need real stimulus pipelines (OpenClaw conversations → mesh stimuli).

**Critical Findings** (accumulated through 01:00 and 01:30 cycles):
- 🔴 **Meta organ bottleneck** — identified at 01:30 (0.266 activation vs 0.463 pressure, 74% pressure ratio)
- ⚠️ **Functional dormancy** — mesh is alive but unconscious (02:30 cycle). Need stimulus pipelines.
- ⚠️ **3 organ scaffolds exist but are empty** — Memory (9030), Reflex (9035), Integration (9036) have Cargo.toml but 0 LOC
- ⚠️ **Mind organ bottleneck** — 0.301 activation vs 0.443 pressure (47% pressure > activation)
- Science dominant at 0.386 activation
- ⚠️ **Three consecutive cycles with zero organ implementation progress**
- ⚠️ **Gateway 1006 error** — WS unreachable, affects cron and remote access
- 4 security warnings: reverse proxy, insecure auth, dangerous config flags, multi-user setup

**Recommendation**: Create Integration organ urgently to relieve Meta organ bottleneck. Memory organ implementation is #1 priority for brain continuity.

## Revised Organ Port Assignments (from 01:30 cycle)

The 01:30 report proposes revised port assignments for new organs (9030-9036) to avoid conflicts with existing services:

| Organ | New Port | Crate | Notes |
|:---|:---|:---|:---|
| Memory | 9030 | `soullink-memory-organ` | Avoids 9016 conflict |
| Perception | 9031 | `soullink-perception-organ` | New port |
| Reasoning | 9032 | `soullink-reasoning-organ` | New port |
| Language | 9033 | `soullink-language-organ` | New port |
| Affect | 9034 | `soullink-affect-organ` | New port |
| Reflex | 9035 | `soullink-reflex-organ` | New port |
| Integration | 9036 | `soullink-integration-organ` | New port |

**Note**: These port assignments (9030-9036) are the latest from 01:30/02:00 cycles and supersede earlier 00:00/00:30 assignments (9016-9023). Orchestrator currently uses 9020 and should migrate to 9030 per these proposals.

## Proposed New Organs

All reports converge on the same set of new organ types. The 22:54 report provides the most detailed specification.

### 1. MEMORY — Long-Term Consolidation (Highest Priority)

| Attribute | Value |
|:---|:---|
| Port | 9016 |
| Neurons | 600 (larger — deep storage capacity) |
| Crate | `soullink-memory` |
| Attractor | DeepBasin (stable repository) |

**Architecture**:
```
soullink-memory/
├── Cargo.toml
├── src/
│   ├── main.rs              — Axum server, port 9016
│   ├── consolidation.rs     — Short-term → long-term memory transfer
│   ├── forgetting.rs        — Decay curves, interference modeling
│   ├── retrieval.rs         — Semantic search, associative recall
│   ├── schema.rs            — Memory types (episodic, semantic, procedural)
│   └── cross_reference.rs   — Build links between memories
```

**Neural Model**:
- Consolidation cycle: Every 15min, pull high-salience stimuli from all nodes
- Forgetting curve: Ebbinghaus-style exponential decay, configurable half-life
- Retrieval: Spreading activation from query → associated memories
- Cross-reference: Build semantic graph connecting related memories
- Attractor: DeepBasin (stable), turbulence-driven recall

**Interfaces**: `POST /stimulus`, `GET /recall?q=&k=`, `POST /consolidate`, `GET /schema`, `POST /forget`

**Emergent Capability**: Without this organ, every session starts from scratch. With it, the brain builds cumulative knowledge that compounds over time. **Highest ROI organ.**

### 2. REFLEX — Fast Reactive Layer

| Attribute | Value |
|:---|:---|
| Port | 9017 |
| Neurons | 100 (small, fast) |
| Crate | `soullink-reflex` |
| Attractor | None (Transient — direct stimulus-response) |

**Architecture**:
```
soullink-reflex/
├── src/
│   ├── main.rs              — Axum server, port 9017
│   ├── pattern_match.rs     — Fast stimulus → response matching
│   ├── safety_guard.rs      — Threat detection, immediate shutdown triggers
│   ├── reflex_arc.rs        — Stimulus-classification → response path
│   └── habit_formation.rs   — Learn new reflexes from repeated patterns
```

**Neural Model**: No attractors — sub-millisecond response time. Habit formation from repeated patterns. Safety override can trigger emergency shutdown.

**Interfaces**: `POST /stimulus`, `GET /reflexes`, `POST /learn`, `POST /override`

**Emergent Capability**: Enables "instinct" — the brain reacts before thinking. **Second highest ROI — safety and responsiveness.**

### 3. INTEGRATION — Cross-Node Synthesis

| Attribute | Value |
|:---|:---|
| Port | 9018 |
| Neurons | 500 |
| Crate | `soullink-integration` |
| Attractor | StrangeAttractor (creative synthesis) |

**Architecture**:
```
soullink-integration/
├── src/
│   ├── main.rs              — Axum server, port 9018
│   ├── synthesis.rs         — Combine outputs from multiple nodes
│   ├── conflict_resolver.rs — Resolve contradictions between node outputs
│   ├── attention.rs         — Attention mechanism for node output weighting
│   └── narrative.rs         — Build coherent narrative from fragmented outputs
```

**Neural Model**: StrangeAttractor for creative synthesis. Dynamic attention weights. Conflict detection and paradox identification. Coherent narrative construction.

**Interfaces**: `POST /synthesize`, `GET /attention`, `POST /resolve`, `GET /narrative`

**Emergent Capability**: The current mesh produces fragmented output. This organ weaves it into coherent thought. **High ROI — directly improves output quality.**

### 4. EMOTION — Valence-Driven Weighting

| Attribute | Value |
|:---|:---|
| Port | 9019 |
| Neurons | 300 |
| Crate | `soullink-emotion` |

**Architecture**:
```
soullink-emotion/
├── src/
│   ├── main.rs              — Axum server, port 9019
│   ├── valence.rs           — Positive/negative stimulus classification
│   ├── arousal.rs           — Energy/urgency assessment
│   ├── mood.rs              — Persistent affective state (turbulence source)
│   └── bias.rs              — Bias signals to other nodes (attention modulation)
```

**Neural Model**: Valence axis (+1 reward to -1 threat). Arousal axis (0 dormant to 1 panic). Persistent mood state. Bias output modulates attention weights in all other nodes.

**Interfaces**: `POST /stimulus`, `GET /mood`, `GET /bias`, `POST /regulate`

**Emergent Capability**: Adds "feeling" to the mesh. Enables urgency detection, threat response, and reward learning. **Medium ROI — enables affective learning.**

### 5. LANGUAGE — Natural Language Understanding/Generation

| Attribute | Value |
|:---|:---|
| Port | 9020 (after orchestrator migrates to 9030) |
| Neurons | 400 |
| Crate | `soullink-language` |

**Architecture**:
```
soullink-language/
├── src/
│   ├── main.rs              — Axum server, port 9020
│   ├── intent.rs            — Intent classification (local, no LLM needed)
│   ├── entity.rs            — Named entity extraction
│   ├── context.rs           — Conversation context management
│   ├── draft.rs             — Response drafting (template + LLM fallback)
│   └── grounding.rs         — Ground language in brain state
```

**Neural Model**: Pattern-based intent matching for fast path. Context window for conversation state. Grounding maps language to brain state. Unknown intents route to LLM with enriched context.

**Interfaces**: `POST /classify`, `POST /extract`, `GET /context`, `POST /draft`

**⚠️ Note:** Port 9020 currently used by orchestrator API. Requires migration to 9030 first.

### 6. REASONING — Logical Inference & Planning

| Attribute | Value |
|:---|:---|
| Port | 9021 |
| Neurons | 350 |
| Crate | `soullink-reasoning` |
| Attractor | StableOrbit (reasoning requires stability) |

**Architecture**:
```
soullink-reasoning/
├── src/
│   ├── main.rs              — Axum server, port 9021
│   ├── deductive.rs         — Logical inference engine
│   ├── planning.rs          — Multi-step plan construction
│   ├── verification.rs     — Consistency checking
│   ├── abduction.rs         — Best-explanation generation
│   └── working_memory.rs    — Scratchpad for active reasoning (7±2 items)
```

**Neural Model**: StableOrbit for stability. Working memory (Miller's law: 7±2). Deductive chain verification. Abductive search with coherence ranking.

**Interfaces**: `POST /infer`, `POST /plan`, `POST /verify`, `GET /hypotheses`

### 7. PERCEPTION — Multi-Modal Input Processing

| Attribute | Value |
|:---|:---|
| Port | 9022 |
| Neurons | 400 |
| Crate | `soullink-perception` |
| Attractor | Transient (rapid state changes) |

**Architecture**:
```
soullink-perception/
├── src/
│   ├── main.rs              — Axum server, port 9022
│   ├── vision.rs            — Image analysis, feature extraction
│   ├── audio.rs             — Audio processing, speech detection
│   ├── structured.rs        — Parse JSON, CSV, tables
│   ├── preprocessing.rs     — Normalize, denoise, segment
│   └── feature_extraction.rs — Convert raw input to neural representations
```

**Emergent Capability**: Enables the brain to "see" and "hear" directly. **Lower ROI initially** — OpenClaw already handles this, but internal processing enables tighter feedback loops.

## Organ Emergence Ranking

From 2026-04-14 00:00 cycle analysis (emergence score = potential for creative, adaptive behavior):

| Rank | Organ | Emergence Score | Rationale |
|:---|:---|:---|:---|
| 1 | **INTEGRATION** (9022) | ⭐⭐⭐⭐⭐ | Unifies all organs into coherent mind. Global Workspace Theory (Baars). |
| 2 | **MEMORY** (9016) | ⭐⭐⭐⭐⭐ | Enables learning across all organs. Hippocampus-inspired consolidation. |
| 3 | **AFFECT/EMOTION** (9021) | ⭐⭐⭐⭐ | Creates adaptive behavioral modes (urgent vs contemplative). Amygdala-inspired. |
| 4 | **LANGUAGE** (9018) | ⭐⭐⭐⭐ | Optimizes primary I/O channel. Broca+Wernicke model. |
| 5 | **REASONING** (9023) | ⭐⭐⭐⭐ | Enables proactive problem-solving. Prefrontal cortex model. |
| 6 | **PERCEPTION** (9017) | ⭐⭐⭐ | Richer input understanding, but downstream value depends on others. |
| 7 | **REFLEX** (9019) | ⭐⭐ | Safety/latency critical, limited creative emergence. |

**Key difference from prior ranking**: Integration promoted to #1 (the "consciousness substrate" — without it, organs are isolated specialists). Memory remains foundational. Affect elevated due to behavioral modulation potential.

## Implementation Phases

Updated from 2026-04-14 00:00 cycle analysis:

**Phase 1 — Foundation (Week 1-2):**
- Implement MEMORY organ (port 9016) — Hippocampus-inspired: fast encoding (CA3 pattern completion) → slow consolidation (CA1 replay)
- Implement REFLEX organ (port 9019) — Monosynaptic reflex arc, <5ms critical safety, <50ms reactive
- Move orchestrator API from 9020 → 9030

**Phase 2 — Cognition (Week 3-4):**
- Implement INTEGRATION organ (port 9022) — Global Workspace Theory, winner-take-all dynamics, recursive self-monitoring
- Implement AFFECT/EMOTION organ (port 9021) — Amygdala-inspired valence, circadian modulation, affect contagion
- Connect MEMORY ↔ all nodes via consolidation cycle

**Phase 3 — Intelligence (Week 5-6):**
- Implement LANGUAGE organ (port 9018) — Broca's (generation) + Wernicke's (comprehension), context window management
- Implement REASONING organ (port 9023) — Prefrontal cortex, forward/backward chaining, constraint satisfaction
- Cross-organ resonance testing

**Phase 4 — Perception (Week 7-8):**
- Implement PERCEPTION organ (port 9017) — Sensory cortex model, cross-modal binding, attention gating
- Multi-modal input pipeline
- Full brain integration testing

## Neural Architecture Improvements

1. **Turbulence Injection**: All nodes at hz=0.0 (dormant). Add scheduled pulses (every 5min) to prevent stagnation.
2. **Cross-Node Resonance**: Nodes operate independently. Add resonant coupling — when science activates, engineer/crypto should feel correlated pressure changes. Phase-lock organs so they synchronize processing cycles.
3. **Attractor Diversity**: Different organs should have different default attractors (Creative→StrangeAttractor, Reasoning→StableOrbit, Reflex→Transient, Memory→DeepBasin, Integration→StrangeAttractor). Add `ChaoticTransition` attractor for mode-switching.
4. **Pressure-Driven Routing**: High-pressure nodes should delegate to lower-pressure neighbors. Route requests to organs based on turbulence (high → creative/brainstorming, low → analytical/reasoning).
5. **Hebbian Learning**: Strengthen connections between organs that fire together. If Memory + Reasoning co-activate frequently, increase their synaptic weight.

## Cross-Node Resonance (Proposed Feature)

**Definition**: Instead of simple orchestrator routing, nodes "echo" signals back and forth to find a stable attractor before committing to a response.

**Key patterns**:
- `creative` + `engineer` synchronization for "flow state" in complex coding tasks
- `meta(9015)` modulating `turbulence` of other nodes based on task complexity
- `affect(9019)` adjusting `turbulence` of `reasoning(9018)` to switch between surgical logic and creative chaos
- Shared memory segments for inter-organ communication (bypass orchestrator)

## Emergent Capability Matrix

| Combination | Emergent Capability |
|:---|:---|
| Persistence + Reasoning | Self-correcting long-term planning |
| Affect + Reflex | Personality and intuitive responses |
| Persistence + Integration (Synapse) | True "Dreaming" state — offline knowledge reorganization |
| Creative + Engineer (Resonance) | Flow state for complex tasks |
| All combined | Proactive goal-seeking (beyond reactive tool-use) |

## Architecture Diagram

```
                    ┌─────────────────────────────────────────┐
                    │          SoulLink Neural Mesh V12       │
                    │        Orchestrator v3 (Rust) :9030    │
                    └──────────────┬──────────────────────────┘
                                   │
        ┌──────────┬──────────┬───┴───┬──────────┬──────────┐
        │          │          │       │          │          │
   ┌────▼───┐ ┌───▼────┐ ┌──▼───┐ ┌─▼────┐ ┌──▼───┐ ┌───▼──┐
   │Science │ │  Mind  │ │Engr  │ │Crypto│ │Creat.│ │ Meta │
   │ :9010  │ │ :9011  │ │:9012 │ │:9013 │ │:9014 │ │:9015│
   │ N=400  │ │ N=400  │ │N=400 │ │N=400 │ │N=400 │ │N=400│
   └────────┘ └────────┘ └──────┘ └──────┘ └──────┘ └─────┘

   ─ ─ ─ PROPOSED NEW ORGANS ─ ─ ─

   ┌────────┐ ┌───────┐ ┌──────────┐ ┌────────┐ ┌───────┐ ┌──────┐ ┌──────┐
   │Memory  │ │Reflex │ │Integrate │ │Emotion│ │Lang.  │ │Reason│ │Percep│
   │ :9016  │ │ :9017 │ │ :9018   │ │ :9019 │ │:9020  │ │:9021 │ │:9022 │
   │ N=600  │ │ N=100 │ │ N=500   │ │ N=300 │ │N=400  │ │N=350 │ │N=400 │
   └────────┘ └───────┘ └──────────┘ └────────┘ └───────┘ └──────┘ └──────┘
```

## Detailed Organ Specifications (from 2026-04-14 00:00 cycle)

### 🧠 Memory Organ (Port 9016)
- **Status**: 🔲 Cargo.toml exists, 0 LOC implementation
- **Neural Architecture**: Hippocampus-inspired: fast encoding (CA3 pattern completion) → slow consolidation (CA1 replay)
- **Forgetting curves**: Ebbinghaus with spaced repetition scheduling
- **Dual-store**: working memory (ephemeral, high Hz) + long-term (RocksDB, persistent)
- **Crate**: `soullink-memory` (Cargo.toml scaffolded)
- **Interfaces**: `POST /stimulus`, `GET /recall`, `POST /consolidate`, `GET /state`, `POST /reinforce`
- **Dependencies**: `axum`, `rocksdb`, `tokio`, `serde`, `chrono`, `uuid`
- **Estimated effort**: 3-4 days for MVP

### 👁️ Perception Organ (Port 9017)
- **Status**: ❌ Not started
- **Neural Architecture**: Sensory cortex model, sensory buffer with attention gating, cocktail party effect
- **Feature extraction** with attention gating, cross-modal binding
- **Crate**: `soullink-perception`
- **Interfaces**: `POST /perceive`, `GET /buffer`, `POST /filter`, `GET /stats`
- **Dependencies**: `axum`, `tokio`, `serde`, `reqwest`
- **Estimated effort**: 3-4 days

### 🗣️ Language Organ (Port 9018)
- **Status**: ❌ Not started
- **Neural Architecture**: Broca's area (generation) + Wernicke's area (comprehension), two-stream processing
- **Token-level attention** with semantic pruning, context window management with priority scoring
- **Crate**: `soullink-language`
- **Interfaces**: `POST /understand`, `POST /generate`, `POST /dialogue`, `GET /sentiment`, `POST /translate`
- **Dependencies**: `axum`, `tokio`, `serde`, `reqwest` (Ollama API), `chrono`
- **Estimated effort**: 4-5 days

### ⚡ Reflex Organ (Port 9019)
- **Status**: 🔲 Cargo.toml exists, 0 LOC implementation
- **Neural Architecture**: Monosynaptic reflex arc, <5ms critical safety, <50ms reactive
- **Pre-compiled pattern table** (no LLM calls, pure computation), if-then production system
- **Crate**: `soullink-reflex` (Cargo.toml scaffolded)
- **Interfaces**: `POST /trigger`, `GET /patterns`, `POST /pattern`, `GET /health`
- **Dependencies**: `axum`, `tokio`, `serde`, `dashmap` (lock-free pattern store)
- **Estimated effort**: 2-3 days for MVP

### 💜 Affect/Emotion Organ (Port 9021)
- **Status**: ❌ Not started
- **Neural Architecture**: Amygdala-inspired: fast valence assignment, 2D affective space (valence × arousal)
- **Circadian modulation**: energy/attention cycles over 24h period
- **Affect contagion**: propagate valence across connected organs
- **Crate**: `soullink-affect`
- **Interfaces**: `POST /evaluate`, `GET /mood`, `POST /bias`, `GET /empathy`, `POST /modulate`
- **Dependencies**: `axum`, `tokio`, `serde`, `rocksdb` (mood persistence)
- **Estimated effort**: 4-5 days

### 🔗 Integration Organ (Port 9022)
- **Status**: 🔲 Cargo.toml exists, 0 LOC implementation
- **⚠️ PRIORITY UPGRADE**: Meta organ bottleneck (74% pressure ratio) requires Integration organ urgently
- **Neural Architecture**: Global Workspace Theory (Baars), winner-take-all dynamics, recursive self-monitoring
- **Directly relieves Meta organ** by offloading cross-organ synthesis
- **Crate**: `soullink-integration` (Cargo.toml scaffolded)
- **Interfaces**: `POST /synthesize`, `GET /metacog`, `POST /attention`, `GET /resonance`, `POST /narrative`
- **Dependencies**: `axum`, `tokio`, `serde`, `reqwest` (inter-organ HTTP), `chrono`
- **Estimated effort**: 5-6 days for MVP

### 🧩 Reasoning Organ (Port 9023)
- **Status**: ❌ Not started
- **Neural Architecture**: Prefrontal cortex model, working memory + rule-based inference
- **Hypothesis generation + testing loop**, causal chain construction
- **Planning** with backtracking and heuristic search
- **⚠️ Port conflict**: Port 9020 currently used by mesh API (returns 404 on /health). Recommend migrating mesh API to 9025.
- **Crate**: `soullink-reasoning`
- **Interfaces**: `POST /infer`, `POST /plan`, `POST /hypothesize`, `GET /constraints`, `POST /evaluate`
- **Dependencies**: `axum`, `tokio`, `serde`, `chrono`
- **Estimated effort**: 5-7 days

## Source Reports

- `night_cycle_20260413_1113.md` — Chronos/Soma/Logos/Thymos/Myelo naming
- `night_cycle_20260413_1115.md` — LTM/Perception/Reasoning/Affect/Reflex/Soma naming
- `night_cycle_20260413_1200.md` — Memory/Perception/Reasoning/Emotion/Reflex naming
- `night_cycle_20260413_1230.md` — Memory/Persistence/Perception/Reasoning/Emotion/Integration naming
- `night_cycle_20260413_1300.md` — Mnemosyne/Aisthesis/Logos/Thymos/Soma/Synapse naming
- `night_cycle_20260413_1430.md` — Chronos/Sensus/Logos/Pathos/Soma naming
- `night_cycle_20260413_1530.md` — Memory/Perception/Reasoning/Affect/Reflex naming
- `night_cycle_20260413_2254.md` — Detailed organ specifications, priority ranking, phases, neural improvements
- `night_cycle_20260414_0000.md` — Full organ specifications with crate structures, emergence ranking, port assignments, API interfaces
- `night_cycle_20260414_0030.md` — Live mesh state (Chaos Initial attractor, turbulence 0.094), Meta organ bottleneck analysis (74% pressure ratio), 3 scaffolded crates, updated organ API specs, port conflict notes
- `night_cycle_20260414_0100.md` — Refined LOC counts, gateway 1006 issue, zero organ progress warning, detailed organ Rust crate structures, priority action matrix
- `night_cycle_20260414_0130.md` — Revised port assignments (9030-9036), detailed organ proposals with crate structures, security warnings, gateway v2026.4.12, soullink-memory v1.0.0 claim (disputed — src/ empty)

## Detailed Organ API Specifications (from 2026-04-14 01:30 and 02:00 cycles)

### Memory Organ (:9030) — Detailed API
- `POST /api/memory/store` — Store a memory (concept, context, strength)
- `POST /api/memory/recall` — Recall by cue (spreading activation)
- `POST /api/memory/consolidate` — Run Ebbinghaus consolidation pass
- `POST /api/memory/forget` — Explicit forgetting
- `GET /api/memory/status` — Memory statistics
- **Neural Model**: Each memory = {concept, embeddings[], strength, last_access, access_count}. Forgetting: Ebbinghaus curve R = e^(-t/S) where S = strength * reinforcement. Consolidation: periodic pass strengthening frequently-accessed memories.
- **Crate structure**: `src/{main.rs, memory.rs, consolidation.rs, recall.rs, archive.rs, routes/{store.rs, recall.rs, consolidate.rs, status.rs}}`

### Reflex Organ (:9035) — Detailed API
- `POST /api/reflex/trigger` — Evaluate stimulus against reflex patterns
- `POST /api/reflex/register` — Register new reflex pattern
- `GET /api/reflex/status` — Active reflexes, trigger counts
- **Neural Model**: Reflex arc pattern→response (<1ms). Built-in: PII guard, destructive action blocker, rate limiter, spam filter. Custom: registered patterns with priority levels.
- **Crate structure**: `src/{main.rs, reflex.rs, patterns.rs, guard.rs, routes/{trigger.rs, register.rs, status.rs}}`

### Integration Organ (:9036) — Detailed API
- `POST /api/integrate` — Run full integration cycle
- `GET /api/integrate/coherence` — Cross-organ coherence score
- `GET /api/integrate/self` — Current self-model
- **Neural Model**: Global Workspace Theory (Baars). All organs broadcast to workspace, attention mechanism selects winner. Coherence checking for contradictions. Self-model tracks capabilities and state.
- **Crate structure**: `src/{main.rs, global_workspace.rs, attention.rs, coherence.rs, self_model.rs, narrative.rs, routes/{integrate.rs, coherence.rs, self_model.rs}}`
- **⚠️ URGENT**: Directly relieves Meta organ bottleneck (74% pressure ratio)

### Reasoning Organ (:9032) — Detailed API
- `POST /api/reason` — Run inference
- `POST /api/reason/plan` — Hierarchical task network planning
- `POST /api/reason/verify` — Self-verification of reasoning chains
- `GET /api/reason/status` — Active reasoning status
- **Neural Model**: Rules stored as {premises[], conclusion, confidence, source}. Forward/backward chaining. Verification via reverse reasoning. Abstraction from repeated patterns.
- **Crate structure**: `src/{main.rs, inference.rs, planner.rs, verify.rs, abstraction.rs, routes/{reason.rs, plan.rs, verify.rs, status.rs}}`

### Perception Organ (:9031) — Detailed API
- `POST /api/perceive` — Multi-modal input processing
- `POST /api/perceive/analyze` — Detailed analysis
- `GET /api/perceive/status` — Perception status
- **Neural Model**: Each modality has processing pipeline with confidence scores. Cross-modal fusion aligns inputs temporally. Outputs structured perception objects consumed by other organs.
- **Crate structure**: `src/{main.rs, vision.rs, audio.rs, text.rs, multimodal.rs, routes/{perceive.rs, analyze.rs, status.rs}}`

### Affect Organ (:9034) — Detailed API
- `POST /api/affect/assess` — Evaluate emotional context
- `GET/POST /api/affect/mood` — Mood tracking and persistence
- `GET /api/affect/status` — Current affective state
- **Neural Model**: 2D emotion space (Valence × Arousal, Russell's circumplex). Drives: curiosity, safety, efficiency, social, mastery. Each drive modulates organ priorities.
- **Crate structure**: `src/{main.rs, valence.rs, arousal.rs, motivation.rs, mood.rs, routes/{assess.rs, mood.rs, status.rs}}`

### Language Organ (:9033) — Detailed API
- `POST /api/language/parse` — Semantic parsing (text → structured meaning)
- `POST /api/language/generate` — Natural language generation (meaning → text)
- `GET /api/language/discourse` — Discourse tracking
- `GET /api/language/status` — Language organ status
- **Neural Model**: Semantic parsing, discourse tracking (topic, intent shifts, grounding). Generation proxies to LLM with enriched context.
- **Crate structure**: `src/{main.rs, understand.rs, generate.rs, discourse.rs, translate.rs, routes/{parse.rs, generate.rs, discourse.rs, status.rs}}`

## Organ Emergence Priority Ranking (03:30 cycle — LATEST)

| Rank | Organ | Port | Neurons | Emergent Value | Effort | ROI | Scaffold Exists? |
|:---|:---|:---|:---|:---|:---|:---|:---|
| 1 | **Memory** | 9021 | 600 | ★★★★★ | Medium | **HIGHEST** | ✅ Yes (empty src/) |
| 2 | **Integration** | 9040 | 500 | ★★★★★ | High | **HIGHEST** | ✅ Yes (empty src/) |
| 3 | **Reflex** | 9030 | 200 | ★★★★ | Low | **VERY HIGH** | ❌ New (03:30 revised) |
| 4 | **Reasoning** | 9022 | 500 | ★★★★ | High | HIGH | ❌ New |
| 5 | **Affect** | 9025 | 300 | ★★★★ | Medium | HIGH | ❌ New |
| 6 | **Perception** | 9023 | 800 | ★★★ | Medium | HIGH | ❌ New |
| 7 | **Language** | 9024 | 400 | ★★ | Medium | MEDIUM | ❌ New |

**Note**: 03:30 cycle revised ports to 9021-9040 and promoted Integration to #2 ("consciousness substrate" — without it, organs are isolated specialists).

## Key Insights (consolidated through 03:30 cycle)

1. **Mesh is structurally alive but functionally dormant.** All 6 nodes sit in DeepBasin with 0.0 hz. The brain exists but nothing is stimulating it. Need real stimulus pipelines (OpenClaw conversations → mesh stimuli).
2. **3 organ scaffolds already exist but are empty.** Memory, Reflex, Integration all have Cargo.toml and empty src/ directories. These are the lowest-hanging fruit.
3. **Gateway WS 1006 is a blocking issue.** Prevents cron management, task operations. Env var points to port 18889 but gateway listens on 18890 — config mismatch.
4. **4 Python processes still running** consume ~87M RAM combined (sl13-mod-evolve alone is 17.4M, and night-cycle-engine Rust binary already exists).
5. **sl13-mod-evolve.py should be killed immediately** — night-cycle-engine Rust binary is compiled and ready.
6. **Zero progress on organ implementations** across 5+ consecutive cycles — scaffolds remain empty.
7. **Stimulus pipeline is the #1 blocker for brain activity.** Orchestrator shows 0 queries. Without wiring OpenClaw events → mesh stimuli, the brain stays dormant regardless of organ count.
8. **Integration organ promoted to #2 priority** — without it, organs are isolated specialists. It creates "consciousness" by binding all organs together.
9. **Synaptic plasticity could be the biggest architectural leap.** Hebbian learning between brain nodes would create emergent processing pathways.
10. **19 Rust crates total** in the ecosystem (6 sub-crates in workspaces), brain stack at 91% compiled.

## Neural Architecture Improvements (03:00 cycle — NEW)

1. **Global attractor field** — Orchestrator maintains global attractor state biasing all nodes
2. **Attractor resonance** — Same-attractor nodes amplify each other (constructive interference)
3. **Phase locking** — Nodes can phase-lock for coordinated deep processing (gamma/beta coupling)
4. **Turbulence cascade** — 6×6 weight matrix: `new_turb[j] = Σ(cascade[i][j] * turb[i])`
5. **Synaptic plasticity (Hebbian)** — `w[i][j] += α * (pre_i * post_j - w[i][j] * decay)`
6. **Turbulence memory** — Track history to predict stability/instability periods
7. **Continuous attractor transitions** — Gradual shifts instead of discrete jumps
8. **Stimulus feed from OpenClaw** — Conversation events → POST /api/mesh/stimulate (#1 blocker)

## Emergent Capability Matrix (03:30 cycle — NEW)

| Organ Pairing | Emergent Property | Value |
|---------------|-------------------|-------|
| Memory + Reasoning | Learning — reason over past experiences | ⭐⭐⭐⭐⭐ |
| Memory + Affect | Emotional Memory — recall with emotional coloring | ⭐⭐⭐⭐ |
| Perception + Language | Understanding — perceive intent behind words | ⭐⭐⭐⭐⭐ |
| Reasoning + Affect | Judgment — logical reasoning with emotional weighting | ⭐⭐⭐⭐ |
| Integration + All | Consciousness — unified experience, self-awareness | ⭐⭐⭐⭐⭐ |
| Reflex + Reasoning | Skill Acquisition — promote successful reasoning to reflexes | ⭐⭐⭐ |
| Creative + Affect | Expression — emotionally-informed creative output | ⭐⭐⭐⭐ |
| Meta + Integration | Self-Improvement — metacognitive optimization loop | ⭐⭐⭐⭐⭐ |

## Source Reports

- `night_cycle_20260413_1113.md` through `night_cycle_20260414_0130.md` — (see above)
- `night_cycle_20260414_0200.md` — Full scope cycle: detailed organ architectures, revised port assignments (9030-9036), emergence priority ranking, neural architecture improvements
- `night_cycle_20260414_0300.md` — v6.0 Extended: 19 Rust crates, Memory organ detailed design (9030), new organ designs (9031-9036), performance optimizations, Hebbian learning, stimulus pipeline proposal, OpenClaw core TS→Rust priority queue
- `night_cycle_20260414_0330.md` — v7.0 Extended: revised port assignments (9021-9040), refined organ designs with crate file layouts, emergent capability matrix, Integration promoted to #2 priority, kill sl13-mod-evolve.py proposal, 91% brain stack compiled

## 05:01 Cycle Updates

### Attractor Landscape Analysis
The mesh has only discovered **1 attractor** ("Chaos Initial", stable). This is critically impoverished. A healthy neural mesh should have 3-5+ attractors representing different cognitive regimes.

**Proposed Attractor Seeding (from 05:01 cycle):**
1. **DeepFocus** — Low turbulence (0.02-0.05), high engineer activation. For deep coding/analysis.
2. **CreativeStorm** — High turbulence (0.15-0.25), high creative activation. For ideation.
3. **CautiousAnalysis** — Low-medium turbulence (0.05-0.08), high science + crypto activation. For risk assessment.
4. **SocialResonance** — Medium turbulence (0.08-0.12), high mind + meta activation. For communication.
5. **ReactiveGuard** — Spiking turbulence (0.20+ transient), high reflex activation. For anomaly response.

### Turbulence Regulation Improvement
Current: hard 0.1 threshold → oscillation around threshold.
**Proposed:**
- **Hysteresis band** (0.08-0.12) instead of hard 0.1 threshold to prevent oscillation
- **Organ-specific turbulence targets** — different organs function best at different turbulence levels
- **Attractor-based regulation** — steer toward known-good attractors rather than generic heat/cool

### Cross-Node Resonance Enhancement
- **Phase-locking**: When two organs are co-active, synchronize processing cycles
- **Inhibitory synapses**: Allow organs to suppress competing organs (avoid resource waste)
- **Resonance amplification**: When multiple organs converge on similar patterns, amplify the signal

### 05:01 Organ Architecture Details

**Recommended implementation order:** Memory → Reflex → Synthesis → Reasoning → Perception → Affect → Language

This ordering maximizes early emergence: Memory enables persistence, Reflex enables speed, Synthesis enables coherence, then higher-order organs build on that foundation.

| Rank | Organ | Port | Emergence Impact | Implementation Effort |
|------|-------|------|-----------------|---------------------|
| 1 | Memory | 9021 | ★★★★★ | Medium (RocksDB pattern exists) |
| 2 | Synthesis | 9027 | ★★★★★ | High (complex cross-organ logic) |
| 3 | Reflex | 9022 | ★★★★☆ | Low (pattern matching is simple) |
| 4 | Reasoning | 9023 | ★★★★★ | High (inference engine is complex) |
| 5 | Perception | 9024 | ★★★★☆ | Medium (multimodal routing) |
| 6 | Affect | 9025 | ★★★☆☆ | Medium |
| 7 | Language | 9026 | ★★★☆☆ | Medium |

**Metrics Summary (from 05:01):**

| Metric | Current | Target (3 cycles) | Target (6 cycles) |
|--------|---------|-------------------|-------------------|
| Rust crates | 12 | 16 | 20 |
| Rust LOC | 6,380 | 10,000+ | 15,000+ |
| Brain organs active | 6 | 8 (+Memory, +Reflex) | 10 (+Synthesis, +Decision) |
| Python processes | 4 | 2 | 0 |
| Discovered attractors | 1 | 5 | 8+ |
| Migration % (brain stack) | 63.6% | 82% | 100% |
| Migration % (OpenClaw core) | 3.3% | 5% | 10% |

## New Organ Proposals (Cycle 131, 14:02)

5 additional organ types proposed for the next evolution wave, complementing the existing 7 proposed organs (9021-9027):

| Rank | Organ | Port | Emergence Score | Neurons | Default Attractor | Purpose |
|------|-------|------|-----------------|---------|-------------------|----------|
| 1 | **soullink-creativity** | 9042 | VERY HIGH | 600 | StrangeAttractor | Cross-domain conceptual combination, metaphor generation, divergent thinking |
| 2 | **soullink-foresight** | 9040 | HIGH | 400 | StableOrbit | Temporal prediction, prefetch cognition, pattern anticipation |
| 3 | **soullink-social** | 9043 | HIGH | 300 | Transient | Theory of mind, communication style adaptation, empathy |
| 4 | **soullink-homeostasis** | 9041 | MEDIUM-HIGH | 200 | DeepBasin | System self-regulation, PID control, thermal management, self-healing |
| 5 | **soullink-validation** | 9044 | MEDIUM | 200 | DeepBasin | Output coherence verification, fact-checking, safety gates, audit trail |

### soullink-foresight (Port 9040)
- **Time-series prediction** engine (exponential smoothing + neural hints)
- **Pattern DB** (RocksDB) for historical sequence storage
- **Prefetch coordinator** — activate resources before demand
- **Temporal extraction** — circadian, weekly, seasonal patterns
- API: `/api/foresight/predict`, `/api/foresight/patterns`, `/api/foresight/prefetch`, `/api/foresight/health`
- **Emergence**: Transforms Integration from reactive → proactive

### soullink-homeostasis (Port 9041)
- **PID controller** for mesh homeostasis
- **Vitals collection** — CPU, memory, latency, queue depth
- **Thermal/charge management** — turbulence → throttling
- **Self-healing** — restart stale organs, rebalance load
- API: `/api/homeostasis/vitals`, `/api/homeostasis/regulate`, `/api/homeostasis/thresholds`, `/api/homeostasis/recover`
- **Emergence**: Transforms mesh into self-regulated system

### soullink-creativity (Port 9042)
- **Conceptual combination** engine (cross-domain blending)
- **Metaphor generation** from concept graphs
- **Divergent thinking** (random walks in concept space)
- **Convergent evaluation** (fitness scoring)
- API: `/api/creativity/combine`, `/api/creativity/metaphor`, `/api/creativity/diverge`, `/api/creativity/evaluate`
- **Emergence**: StrangeAttractor produces most unpredictable outputs

### soullink-social (Port 9043)
- **Theory of mind** — model interlocutor mental states
- **Style adapter** — communication style matching
- **Social context** — group dynamics, hierarchies
- **Affective empathy** — linked to Affect organ
- API: `/api/social/model`, `/api/social/adapt`, `/api/social/context`, `/api/social/profile`
- **Emergence**: Massively enriches human interactions

### soullink-validation (Port 9044)
- **Output coherence verification**
- **Fact-checking** — cross-reference validation (links to Perception/Reasoning)
- **Safety boundary enforcement**
- **Immutable audit trail** (RocksDB, SHA-256)
- API: `/api/validation/verify`, `/api/validation/factcheck`, `/api/validation/safety`, `/api/validation/audit`
- **Emergence**: Critical for reliability and trust

### Architecture Improvement Proposals (14:02)

1. **Cross-Organ Resonance Protocol** — UDP multicast pub/sub for simultaneous multi-organ activation + attractor synchronization (HIGH priority)
2. **Dynamic Neuron Allocation** — 200-800 range based on load, via Homeostasis organ (MEDIUM)
3. **Attractor Transition Logging** — journal of DeepBasin→StrangeAttractor transitions (LOW, observability)
4. **Hierarchical Mesh** — clustering by Cognitive/Physical/Meta groups (MEDIUM)
5. **RocksDB Column Families** for Memory organ — separate episodic/semantic/procedural stores
6. **Connection pooling** between organs — persistent HTTP/2
7. **Batch API** for Integration organ — group cross-organ requests
8. **SIMD** for soullink-math — vectorize matrix operations

### Security Proposals (14:02) — ⚠️ Manual Review Required

1. **mTLS between organs** — mutual certificates on internal mesh
2. **Audit trail** — via Validation organ (immutable RocksDB)
3. **Rate limiting per-organ** — prevent cascade failure
4. **Secret rotation** — RocksDB encrypted column family

### Cron Issue (14:02)

⚠️ `nightly-insights` cron job failing — likely kimi-k2.5 model unavailable. Needs manual fix or model change.

## Cycle 131 Extended Organ Proposals (14:02)

5 additional organ proposals beyond the original 7, with their own emergence ranking:

| Rank | Organ | Port | Score | Default Attractor | Key Capability |
|:---|:---|:---|:---|:---|:---|
| 1 | **Creativity** | 9042 | VERY HIGH | StrangeAttractor | Cross-domain concept combination, metaphor generation, divergent thinking |
| 2 | **Foresight** | 9040 | HIGH | StableOrbit | Predictive prefetch, temporal pattern extraction, proactive resource allocation |
| 3 | **Social** | 9043 | HIGH | Transient | Theory of mind, communication style adaptation, group dynamics |
| 4 | **Homeostasis** | 9041 | MEDIUM-HIGH | DeepBasin | PID controller, self-healing, throttling, dynamic neuron allocation |
| 5 | **Validation** | 9044 | MEDIUM | DeepBasin | Output coherence, fact-checking, safety gates, immutable audit trail |

See individual proposal docs: `soullink_foresight_organ.md`, `soullink_homeostasis_organ.md`, `soullink_creativity_organ.md`, `soullink_social_organ.md`, `soullink_validation_organ.md`

### Architecture Improvement Docs (from Cycle 131)

New documentation created:
- `dynamic_neuron_allocation.md` — 200-800 range per node, managed by Homeostasis organ
- `attractor_transition_logging.md` — Journal of regime changes per node
- `hierarchical_mesh_architecture.md` — Cognitive/Physical/Meta group clustering
- `rocksdb_column_families_memory.md` — Per-type RocksDB CFs for Memory organ
- `integration_batch_api.md` — Parallel fan-out for multi-organ synthesis
- `simd_vectorization_soullink_math.md` — AVX2/AVX-512 for 5-8x math speedup
- `audit_trail_immutable.md` — SHA-256 chained audit log for Validation organ

## Source Reports

- `night_cycle_20260413_1113.md` through `night_cycle_20260414_0330.md` — (see above)
- `night_cycle_20260414_0501.md` — v7.0 Extended: 12 crates/6,380 LOC, brain stack 63.6%, Meta bottleneck (0.463 pressure), Chaos Initial attractor (only 1), 7 organ architectures (9021-9027), emergence ranking, soullink-server-core force multiplier, attractor seeding (5 proposed), hysteresis turbulence regulation, cross-node resonance enhancement
- `night_cycle_20260414_1402.md` — Cycle 131: 5 new organ proposals (foresight/homeostasis/creativity/social/validation, ports 9040-9044), emergence ranking, architecture improvements (resonance/dynamic neurons/hierarchical mesh), security proposals (mTLS/audit/rate-limit/secret-rotation), Rust migration 18/18 SoulLink complete, OpenClaw TS→Rust 0/60, nightly-insights cron error

## Last Updated

2026-04-14T15:26:00+02:00 — Auto-apply cycle (14:02 report cycle 131: 5 new organ proposals, 7 new architecture improvement docs, emergence ranking, cron error tracking)