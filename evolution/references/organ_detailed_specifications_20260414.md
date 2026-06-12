# Organ Detailed Specifications (2026-04-14)

> Extracted from night_cycle_20260414_0230.md — Section 4: New Organ Proposals with detailed architectures

## Port Allocation

| Port | Organ | Status |
|------|-------|--------|
| 9010 | science | ✅ Active v6.1 |
| 9011 | mind | ✅ Active v6.1 |
| 9012 | engineer | ✅ Active v6.1 |
| 9013 | crypto | ✅ Active v6.1 |
| 9014 | creative | ✅ Active v6.1 |
| 9015 | meta | ✅ Active v6.1 |
| 9020 | orchestrator | ✅ Active v3.0 |
| 9030 | memory | 🆕 Proposed (Cargo.toml exists) |
| 9031 | reflex | 🆕 Proposed (scaffold exists) |
| 9032 | integration | 🆕 Proposed (scaffold exists) |
| 9033 | perception | 🆕 Proposed (NEW) |
| 9034 | language | 🆕 Proposed (NEW) |
| 9035 | affect | 🆕 Proposed (NEW) |
| 9036 | reasoning | 🆕 Proposed (NEW) |

## Organ 1: MEMORY (Long-term Consolidation)

**Port:** 9030  
**Crate:** `soullink-memory` v1.0.0 (Cargo.toml exists, src/ empty)  
**Impact:** 🔴 CRITICAL — Without persistence, all learning is ephemeral  
**Build Time:** 4-6 hours

### Architecture

```
├── src/main.rs          — Axum server, port 9030
├── src/memory/
│   ├── mod.rs           — MemoryStore trait
│   ├── consolidation.rs — Ebbinghaus forgetting curve implementation
│   ├── recall.rs        — Weighted recall with context boosting
│   ├── decay.rs         — Time-based memory decay, sleep consolidation
│   └── index.rs         — Semantic indexing over stored memories
├── src/api/
│   ├── mod.rs           — Router
│   ├── store.rs         — POST /api/memory/store
│   ├── recall.rs        — POST /api/memory/recall
│   ├── consolidate.rs   — POST /api/memory/consolidate
│   └── stats.rs         — GET /api/memory/stats
└── src/persistence/
    ├── mod.rs           — RocksDB layer
    └── migration.rs     — Schema versioning
```

### API Endpoints

- `POST /api/memory/store` — Store a memory with tags and strength
- `POST /api/memory/recall` — Recall memories matching context (spreading activation)
- `POST /api/memory/consolidate` — Trigger consolidation cycle
- `POST /api/memory/forget` — Explicit forgetting
- `GET /api/memory/stats` — Memory count, avg strength, decay distribution

### Neural Model

- 512 neurons arranged as "memory engram" cells
- Each engram: `{concept, strength, last_accessed, creation_ts, decay_rate}`
- Forgetting: Ebbinghaus curve R = e^(-t/S) where S = stability × reinforcement count
- Consolidation: during low-turbulence periods, replay important memories to strengthen
- Recall: spreading activation — query primes related memories
- Dependencies: axum, tokio, serde, rocksdb, chrono, uuid

### Emergent Value: ★★★★★

Foundation for everything else. Without memory, the brain resets every session.

---

## Organ 2: REFLEX (Fast Reactive Responses)

**Port:** 9031  
**Crate:** `soullink-reflex` (scaffold exists)  
**Impact:** 🟡 HIGH — Major latency and cost reduction  
**Build Time:** 3-4 hours

### Architecture

```
├── src/main.rs           — Axum server, port 9031
├── src/reflex/
│   ├── mod.rs            — ReflexArc trait
│   ├── pattern_match.rs  — Regex/keyword pattern matching
│   ├── threat_detect.rs  — Anomaly/threat scoring
│   ├── auto_response.rs  — Pre-programmed reflex responses
│   └── habit.rs          — Learned habitual patterns
├── src/api/
│   ├── mod.rs            — Router
│   ├── trigger.rs        — POST /api/reflex/trigger
│   ├── learn.rs          — POST /api/reflex/learn (new pattern)
│   └── status.rs         — GET /api/reflex/status
└── src/persistence/
    └── mod.rs            — RocksDB for pattern cache
```

### API Endpoints

- `POST /api/reflex/trigger` — Evaluate input for reflex match
- `POST /api/reflex/learn` — Learn a new reflex pattern
- `GET /api/reflex/status` — Active reflexes count, recent triggers

### Neural Model

- 200 neurons with very low threshold (0.3 vs standard 1.0)
- No leak — instant activation
- "Shortcut" pathways: input → direct motor output, bypassing deliberation
- Spike-timing dependent plasticity (STDP) for learning new reflexes
- Competes with slower organs — if reflex fires, suppresses deeper processing
- Dependencies: axum, tokio, serde, serde_json

### Emergent Value: ★★★★

Sub-100ms response to known patterns. Instant safety + responsiveness without LLM token burn.

---

## Organ 3: INTEGRATION (Cross-Node Synthesis / Meta-cognition)

**Port:** 9032  
**Crate:** `soullink-integration` (scaffold exists)  
**Impact:** 🟡 HIGH — Enables emergent intelligence across organs  
**Build Time:** 5-7 hours

### Architecture

```
├── src/main.rs           — Axum server, port 9032
├── src/integration/
│   ├── mod.rs            — IntegrationEngine trait
│   ├── resonance.rs      — Cross-organ coherence detection
│   ├── meta_monitor.rs   — Monitor all organ states, detect anomalies
│   ├── synthesis.rs      — Combine outputs from multiple organs
│   └── self_model.rs     — Maintain a "self-model" of the system
├── src/api/
│   ├── mod.rs            — Router
│   ├── observe.rs        — GET /api/integration/observe
│   ├── synthesize.rs     — POST /api/integration/synthesize
│   └── self_model.rs     — GET /api/integration/self-model
└── src/persistence/
    └── mod.rs            — RocksDB
```

### API Endpoints

- `GET /api/integration/observe` — Current cross-organ state
- `POST /api/integration/synthesize` — Force synthesis of current inputs
- `GET /api/integration/self-model` — Self-model JSON

### Neural Model

- 256 neurons, each connected to a "readout" from every other organ
- Implements "global workspace" theory: when multiple organs activate simultaneously, integration organ amplifies the signal (consciousness)
- Resonance detection: if science + crypto both spike → market-research insight
- Self-model: maintains a JSON representation of "who I am and what I know" updated every consolidation cycle
- Dependencies: axum, tokio, serde, serde_json, reqwest

### Emergent Value: ★★★★★

True meta-cognition. The system can observe its own processing, detect confusion (high turbulence + low confidence), and trigger corrective actions. The "consciousness" layer.

---

## Organ 4: PERCEPTION (Multi-Modal Input Processing)

**Port:** 9033  
**Crate:** `soullink-perception` (NEW)  
**Impact:** 🟢 MEDIUM — Better routing, fewer wasted tokens  
**Build Time:** 4-5 hours

### Architecture

```
├── src/main.rs
├── src/perception/
│   ├── mod.rs            — PerceptionEngine trait
│   ├── text_preproc.rs   — Tokenization, language detection, intent classification
│   ├── image_preproc.rs  — Feature extraction hooks (delegates to external models)
│   ├── audio_preproc.rs  — Audio feature hooks (delegates to whisper)
│   ├── saliency.rs       — Saliency scoring — what deserves attention?
│   └── router.rs         — Route processed input to appropriate organ(s)
├── src/api/
│   ├── mod.rs
│   ├── perceive.rs       — POST /api/perception/process
│   └── stats.rs          — GET /api/perception/stats
```

### Neural Model

- 300 neurons as "sensory cortex"
- Topographic organization: different regions specialize for different modalities
- Saliency gate: only passes stimuli above threshold to deeper processing
- "Attention spotlight": amplifies one modality at a time
- Dependencies: axum, tokio, serde, serde_json, reqwest, base64

### Emergent Value: ★★★

Unified perception pipeline. Pre-classifies intent, enriches context, and routes to the right brain region.

---

## Organ 5: LANGUAGE (Natural Language Understanding/Generation)

**Port:** 9034  
**Crate:** `soullink-language` (NEW)  
**Impact:** 🟢 MEDIUM — Better context management  
**Build Time:** 5-6 hours

### Architecture

```
├── src/main.rs
├── src/language/
│   ├── mod.rs
│   ├── context_window.rs — Manage conversation context, sliding window
│   ├── intent_parser.rs  — Extract intent from text (rule-based + neural)
│   ├── tone_adjust.rs    — Adjust output tone based on turbulence
│   ├── multilingual.rs   — Language detection and routing
│   └── token_budget.rs  — Track and optimize token usage
├── src/api/
│   ├── mod.rs
│   ├── parse.rs          — POST /api/language/parse
│   ├── generate.rs       — POST /api/language/generate-hint
│   └── stats.rs          — GET /api/language/stats
```

### Neural Model

- 350 neurons as "Broca/Wernicke" analogs
- Language-conditional activation: French input activates different neuron populations than English
- Context window: maintains a compressed representation of conversation history
- SOULLINK V12 integration: directly reads turbulence and adjusts output tone (turbulent → creative/staccato, calm → analytical)
- Dependencies: axum, tokio, serde, serde_json, reqwest

### Emergent Value: ★★

LLMs already provide most language capability. Value is in persistent discourse state and tone adaptation.

---

## Organ 6: AFFECT/EMOTION (Valence-Driven Decision Weighting)

**Port:** 9035  
**Crate:** `soullink-affect` (NEW)  
**Impact:** 🟡 HIGH — Creates self-regulating behavior  
**Build Time:** 3-4 hours

### Architecture

```
├── src/main.rs
├── src/affect/
│   ├── mod.rs
│   ├── valence_tracker.rs — Track positive/negative valence over time
│   ├── mood_engine.rs     — Compute system mood (curious, anxious, calm, urgent)
│   ├── decision_weight.rs — Modulate organ decisions based on affect
│   └── circadian.rs       — Time-of-day modulation (night = lower arousal)
├── src/api/
│   ├── mod.rs
│   ├── valence.rs         — GET /api/affect/valence
│   ├── mood.rs            — GET /api/affect/mood
│   └── modulate.rs        — POST /api/affect/modulate
```

### Neural Model

- 150 neurons as "amygdala" analog
- Dopamine/serotonin analogs: reward signals increase "dopamine", failures increase "serotonin" (caution)
- Mood = running average of valence × time
- Circadian rhythm: arousal naturally dips at night, peaks mid-morning
- Decision modulation: high dopamine → more exploratory, high serotonin → more cautious
- Dependencies: axum, tokio, serde, serde_json

### Emergent Value: ★★★

Self-regulating behavior. The system develops "preferences" and "moods" that persist across sessions. More cautious after failures, more creative after successes.

---

## Organ 7: REASONING (Logical Inference & Planning)

**Port:** 9036  
**Crate:** `soullink-reasoning` (NEW)  
**Impact:** 🟢 MEDIUM — Useful for complex multi-step tasks  
**Build Time:** 6-8 hours

### Architecture

```
├── src/main.rs
├── src/reasoning/
│   ├── mod.rs
│   ├── inference.rs       — Rule-based forward chaining
│   ├── planner.rs         — HTN (Hierarchical Task Network) planner
│   ├── goal_tracker.rs    — Track active goals and sub-goals
│   └── constraint.rs      — Constraint satisfaction
├── src/api/
│   ├── mod.rs
│   ├── infer.rs           — POST /api/reasoning/infer
│   ├── plan.rs            — POST /api/reasoning/plan
│   └── goals.rs           — GET /api/reasoning/goals
```

### Neural Model

- 300 neurons as "prefrontal cortex" analog
- Working memory: maintains current goal stack
- Inference chains: neurons that fire in sequence represent logical steps
- Planner: decomposes complex goals into sub-goals using learned patterns
- Self-correction: if a plan fails, strengthens "avoid" pathway
- Dependencies: axum, tokio, serde, serde_json

### Emergent Value: ★★★★

Structured, verifiable reasoning beyond LLM token prediction. Multi-step planning without burning LLM tokens.

---

## Emergence Priority Ranking

| Rank | Organ | Why It Creates Most Emergence |
|------|-------|------------------------------|
| 🥇 | **MEMORY** | Without persistence, all other organs reset on restart. Memory enables learning accumulation. |
| 🥈 | **INTEGRATION** | Cross-organ synthesis is where intelligence emerges. Without it, organs are isolated islands. |
| 🥉 | **AFFECT** | Self-regulation creates adaptive behavior. Affect + memory = personality. |
| 4 | **REFLEX** | Instant pattern matching reduces latency and cost. Most "practical" value. |
| 5 | **PERCEPTION** | Better input routing helps all organs, but requires downstream organs to benefit. |
| 6 | **LANGUAGE** | Context management is useful but mostly handled by OpenClaw's context engine. |
| 7 | **REASONING** | Powerful but complex. Can be approximated by LLM + planning skills. |

## Neural Architecture Improvements (from 02:30 report)

### 1. Resonance Engine
When multiple organs simultaneously transition from DeepBasin → StableOrbit, trigger a "resonance event" that amplifies all connected organs. Mimics gamma-wave synchronization in biological brains.

### 2. Cross-Node Gradient Propagation
Currently nodes propagate only spike counts. Propagate full gradient (weight deltas) so learning in one organ influences connected organs.

### 3. Attractor Landscape
Add secondary attractor type — "LimitCycle" — for oscillatory patterns. Enables rhythmic processing (circadian modulation, periodic consolidation).

### 4. Turbulence-Modulated Routing
When turbulence is high (StrangeAttractor), route input to creative/meta organs. When low (DeepBasin), route to science/engineer for precise processing. This is the brain's "attention mechanism."

### 5. Sleep Consolidation
When all organs are in DeepBasin for >30 min, trigger a "sleep cycle" where the integration organ replays the day's stimuli to strengthen important pathways and prune weak ones.

---

*Extracted from night_cycle_20260414_0230.md by auto-apply cycle at 2026-04-14T02:38*