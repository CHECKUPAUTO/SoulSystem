# New Organ Proposals V2 (2026-04-14, Cycle 131)

> From night_cycle_20260414_1402.md — 5 new organ types complementing existing 7 proposed organs (9021-9027)

## Emergence Ranking

| Rank | Organ | Port | Emergence Score | Neurons | Default Attractor | Rationale |
|------|-------|------|-----------------|---------|-------------------|-----------|
| 1 | **soullink-creativity** | 9042 | VERY HIGH | 600 | StrangeAttractor | Cross-domain combination + StrangeAttractor = radically new outputs |
| 2 | **soullink-foresight** | 9040 | HIGH | 400 | StableOrbit | Transforms Integration from reactive → proactive |
| 3 | **soullink-social** | 9043 | HIGH | 300 | Transient | Massively enriches human interactions |
| 4 | **soullink-homeostasis** | 9041 | MEDIUM-HIGH | 200 | DeepBasin | Auto-regulation, mostly stabilization not emergence |
| 5 | **soullink-validation** | 9044 | MEDIUM | 200 | DeepBasin | Critical for reliability/trust, but low creative emergence |

---

## 1. soullink-foresight (Port 9040)

**Type:** Anticipation / Prediction  
**Purpose:** Predict future needs based on historical patterns. Activate resources before demand (cognitive prefetch).

### Architecture

```
soullink-foresight/
├── src/main.rs          — Axum HTTP server, port 9040
├── src/predictor.rs     — Time-series prediction engine (exponential smoothing + neural hints)
├── src/pattern_db.rs    — RocksDB pattern storage (historical sequences)
├── src/prefetch.rs      — Resource prefetch coordinator
├── src/temporal.rs      — Temporal pattern extraction (circadian, weekly, seasonal)
└── Cargo.toml           — axum, tokio, rocksdb, chrono, serde
```

### API Interfaces

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/api/foresight/predict` | Prediction for a given context |
| GET | `/api/foresight/patterns` | Identified temporal patterns |
| POST | `/api/foresight/prefetch` | Trigger resource prefetch |
| GET | `/api/foresight/health` | Health check |

### Neural Configuration

- **Neurons**: 400
- **Default Attractor**: StableOrbit (predictable, recurrent patterns)
- **Emergence**: HIGH — enables Integration organ to shift from reactive to proactive

---

## 2. soullink-homeostasis (Port 9041)

**Type:** System Self-Regulation  
**Purpose:** Maintain global mesh equilibrium. Monitor load, latency, coherence. Trigger throttling or scaling.

### Architecture

```
soullink-homeostasis/
├── src/main.rs          — Axum HTTP server, port 9041
├── src/regulator.rs     — PID controller for mesh homeostasis
├── src/vitals.rs        — System vitals collection (CPU, mem, latency, queue depth)
├── src/thermostat.rs    — Thermal/charge management (turbulence → throttling)
├── src/recovery.rs      — Self-healing: restart stale organs, rebalance load
└── Cargo.toml           — axum, tokio, sysinfo, serde, tracing
```

### API Interfaces

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/api/homeostasis/vitals` | Current system vitals |
| POST | `/api/homeostasis/regulate` | Trigger regulation cycle |
| GET | `/api/homeostasis/thresholds` | Current regulation thresholds |
| POST | `/api/homeostasis/recover` | Trigger self-healing |

### Neural Configuration

- **Neurons**: 200
- **Default Attractor**: DeepBasin (calm, receptive)
- **Emergence**: MEDIUM-HIGH — transforms mesh into self-regulated system

---

## 3. soullink-creativity (Port 9042)

**Type:** Emergent Creative Generation  
**Purpose:** Conceptual combination, cross-metaphor, lateral thinking. Complements the existing "creative" node (9014).

### Architecture

```
soullink-creativity/
├── src/main.rs          — Axum HTTP server, port 9042
├── src/combinator.rs    — Conceptual combination engine (cross-domain blending)
├── src/metaphor.rs      — Metaphor generation from concept graphs
├── src/divergence.rs    — Divergent thinking module (random walks in concept space)
├── src/convergence.rs   — Convergent evaluation (fitness scoring)
├── src/concept_db.rs    — RocksDB concept graph storage
└── Cargo.toml           — axum, tokio, rocksdb, rand, serde
```

### API Interfaces

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/api/creativity/combine` | Combine concepts across domains |
| POST | `/api/creativity/metaphor` | Generate metaphor for concept pair |
| POST | `/api/creativity/diverge` | Divergent exploration from seed |
| POST | `/api/creativity/evaluate` | Score creative output fitness |

### Neural Configuration

- **Neurons**: 600 (largest of all proposed organs)
- **Default Attractor**: StrangeAttractor (produces most unpredictable outputs)
- **Emergence**: VERY HIGH — StrangeAttractor + cross-domain blending = radically novel outputs

---

## 4. soullink-social (Port 9043)

**Type:** Social Intelligence / Theory of Mind  
**Purpose:** Model interlocutor mental states. Adapt tone, style, timing. Critical for multi-user interactions.

### Architecture

```
soullink-social/
├── src/main.rs          — Axum HTTP server, port 9043
├── src/theory_of_mind.rs — Mental state modeling for interlocutors
├── src/style_adapter.rs  — Communication style matching
├── src/context_social.rs — Social context tracking (group dynamics, hierarchies)
├── src/empathy.rs        — Affective empathy simulation (linked to Affect organ)
├── src/social_db.rs      — RocksDB interlocutor profiles
└── Cargo.toml           — axum, tokio, rocksdb, serde, chrono
```

### API Interfaces

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/api/social/model` | Model interlocutor mental state |
| POST | `/api/social/adapt` | Adapt response style to interlocutor |
| GET | `/api/social/context` | Current social context |
| POST | `/api/social/profile` | Update interlocutor profile |

### Neural Configuration

- **Neurons**: 300
- **Default Attractor**: Transient (adapting, transitioning between states)
- **Emergence**: HIGH — massively enriches human interactions

---

## 5. soullink-validation (Port 9044)

**Type:** Internal Verification / Audit  
**Purpose:** Before any external output, validate coherence, fact-check, safety. "Critical conscience" of the system.

### Architecture

```
soullink-validation/
├── src/main.rs          — Axum HTTP server, port 9044
├── src/verifier.rs      — Output coherence verification
├── src/fact_check.rs    — Cross-reference validation (link to Perception/Reasoning)
├── src/safety_gate.rs   — Safety boundary enforcement
├── src/audit_log.rs     — RocksDB immutable audit trail
├── src/critic.rs        — Internal critic scoring
└── Cargo.toml           — axum, tokio, rocksdb, serde, sha2
```

### API Interfaces

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/api/validation/verify` | Verify output coherence |
| POST | `/api/validation/factcheck` | Fact-check claim |
| POST | `/api/validation/safety` | Safety boundary check |
| GET | `/api/validation/audit` | Audit trail query |

### Neural Configuration

- **Neurons**: 200
- **Default Attractor**: DeepBasin (calm, methodical)
- **Emergence**: MEDIUM — but HIGH for reliability/trust

---

## Implementation Priority

Recommended order based on emergence ROI:

1. **soullink-creativity** (9042) — Highest emergence, StrangeAttractor enables radical novelty
2. **soullink-foresight** (9040) — Enables proactive behavior, pairs well with Integration
3. **soullink-social** (9043) — Richer human interaction, theory of mind
4. **soullink-homeostasis** (9041) — System stability, enables dynamic neuron allocation
5. **soullink-validation** (9044) — Safety net, pairs with security hardening

## Relationship to Existing Proposals

These 5 organs (ports 9040-9044) complement the previously proposed 7 organs (ports 9021-9027):
- **Foresight** synergizes with Integration (9027/9036) — prefetch before synthesis
- **Homeostasis** synergizes with all organs — global regulation
- **Creativity** synergizes with Creative node (9014) — organ-level creative engine vs node-level
- **Social** synergizes with Affect (9025/9034) — empathy + theory of mind
- **Validation** synergizes with Perception (9024/9033) and Reasoning (9023/9031) — verification + fact-check

## Source

- `night_cycle_20260414_1402.md` — Cycle 131, turbulence 0.094, Chaos Initial attractor

## Last Updated

2026-04-14T14:07:00+02:00 — Auto-apply cycle