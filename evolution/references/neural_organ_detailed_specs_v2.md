# Neural Organ Detailed Specifications v2

**Source:** Night cycle reports 2026-04-14 (00:00 through 05:01)
**Created:** 2026-04-14 by auto-apply
**Status:** Reference documentation (no code changes)

---

## Organ Architecture Overview

All organs follow the `soullink-node` pattern (axum + RocksDB + health endpoint).
New organs should use the `soullink-server-core` shared library (proposed) to reduce boilerplate by ~60%.

### Port Allocation (Updated)

| Organ | Port | Status | Priority | Emergence Score |
|-------|------|--------|----------|-----------------|
| Science | 9010 | ✅ Production | — | — |
| Mind | 9011 | ✅ Production | — | — |
| Engineer | 9012 | ✅ Production | — | — |
| Crypto | 9013 | ✅ Production | — | — |
| Creative | 9014 | ✅ Production | — | — |
| Meta | 9015 | ✅ Production | — | — |
| Orchestrator | 9020 | ✅ Production | — | — |
| **Memory** | 9021/9017/9030 | 🔶 Skeleton | P0 | ★★★★★ |
| **Reflex** | 9022/9018/9035 | 🔶 Skeleton | P0-P1 | ★★★★☆ |
| **Integration** | 9027/9016/9036 | 🔶 Skeleton | P0 | ★★★★★ |
| **Reasoning** | 9023/9032 | ❌ New | P2 | ★★★★★ |
| **Perception** | 9024/9019/9031 | ❌ New | P1 | ★★★★☆ |
| **Affect** | 9025/9034 | ❌ New | P2 | ★★★☆☆ |
| **Language** | 9026/9018/9033 | ❌ New | P3 | ★★★☆☆ |
| **Decision** | — | ❌ New | P1 | ★★★★☆ |

> Note: Port assignments vary across reports. Final allocation needs consolidation.

---

## Memory Organ (soullink-memory) — P0 CRITICAL

**Highest priority.** Without persistence, every session is amnesiac.

### Architecture
```
soullink-memory/
├── Cargo.toml
├── src/
│   ├── main.rs          — axum server, port 9021
│   ├── consolidation.rs — Ebbinghaus forgetting curve, spaced repetition
│   ├── recall.rs        — Semantic similarity search via embeddings
│   ├── decay.rs         — Time-weighted importance scoring
│   └── synapse.rs       — Cross-organ memory sharing protocol
```

### Key Algorithms
- **Ebbinghaus Forgetting Curve**: `R(t) = e^(-t/S)` where S = strength, decays with time
- **Spaced Repetition**: Scheduler based on activation history
- **Consolidation**: Short-term → long-term transfer during low-turbulence states
- **Decay**: Low-utility memories fade; high-retrieval-count memories strengthen

### API Endpoints
- `POST /api/consolidate` — Trigger consolidation cycle
- `GET /api/recall?q={query}&k={count}` — Semantic recall
- `POST /api/store` — Store new memory with metadata
- `GET /api/health` — Standard health check
- `POST /api/decay` — Run decay pass

### Dependencies
- `axum`, `tokio`, `serde`, `rocksdb` (already in Cargo.toml), `chrono`, `uuid`

### Implementation Order
1. `store.rs` → `recall.rs` → `consolidation.rs` → `server.rs`
2. Estimated: 800 LOC, 2-3 days focused work

---

## Reflex Organ (soullink-reflex) — P0/P1

### Architecture
```
soullink-reflex/
├── Cargo.toml
├── src/
│   ├── main.rs        — axum server, port 9022
│   ├── pattern.rs     — Pre-compiled reflex patterns (match → action)
│   ├── fast_path.rs   — Zero-allocation hot path for known triggers
│   ├── guard.rs       — Safety reflexes (rate limits, anomaly detection)
│   └── route.rs       — Pattern-based routing to other organs
```

### Key Design Principles
- **Sub-5ms response** — No LLM calls, no DB writes on fast path
- **Pattern matching only** — Regex/trie-based trigger recognition
- **Guard reflexes** — Rate limiting, anomaly detection, circuit breaking
- **Routing reflexes** — Classify incoming signal → forward to appropriate organ

### API Endpoints
- `POST /api/trigger` — Input signal for reflex evaluation
- `GET /api/patterns` — List loaded reflex patterns
- `POST /api/patterns` — Add/update reflex pattern
- `GET /api/health`

### Estimated: 400 LOC, 2-3 days

---

## Integration Organ (soullink-synthesis/integration) — P0

**Directly addresses Meta bottleneck** (pressure 0.463, activation 0.266).

### Architecture
```
soullink-synthesis/
├── Cargo.toml
├── src/
│   ├── main.rs           — axum server, port 9027
│   ├── cross_modal.rs    — Fuse inputs from multiple organs
│   ├── conflict.rs       — Resolve contradictory organ outputs
│   ├── meta_cognition.rs — Self-model, introspection
│   ├── emergence.rs      — Detect emergent patterns across organ states
│   └── narrative.rs      — Construct coherent narrative from organ states
```

### Critical Design Insight
Meta node (9015) currently has the highest pressure but lowest activation — it's overwhelmed trying to integrate without a dedicated synthesis organ. `soullink-synthesis` would absorb the cross-node integration burden, freeing Meta for self-regulation.

### Estimated: 1200 LOC, 3-4 days

---

## Affect Organ (soullink-affect) — P2

### Architecture
```
soullink-affect/
├── Cargo.toml
├── src/
│   ├── main.rs          — HTTP server (axum, port 9025)
│   ├── valence.rs       — Positive/negative valence scoring
│   ├── arousal.rs       — Activation level tracking
│   ├── sentiment.rs     — Text sentiment analysis (local model)
│   ├── weighting.rs     — Decision weight modulation based on affect
│   └── resonance.rs     — Cross-organ affect propagation
```

### Key Design
- **Valence axis**: -1.0 (negative/avoidant) to +1.0 (positive/approach)
- **Arousal axis**: 0.0 (calm) to 1.0 (activated/urgent)
- **Weighting**: High-arousal negative → risk aversion; high-arousal positive → opportunity seeking
- **Resonance**: Affect state modulates other organs' processing parameters

### Estimated: 500 LOC, 4-5 days

---

## soullink-server-core — Force Multiplier

**Highest leverage proposal.** Currently every node re-implements axum+rocksdb+health boilerplate.
A shared library would cut new organ implementation time by ~60%.

### Extraction Targets
- axum server setup + graceful shutdown
- RocksDB initialization + column families
- Health check endpoint pattern
- Configuration loading from environment
- Logging/metrics setup
- Error types and JSON response helpers

### Estimated: 300-400 LOC, 1-2 days

---

## Stimulus Pipeline — Critical Missing Piece

The mesh is structurally alive but functionally dormant (Hz=0.0, all nodes in DeepBasin).
The #1 blocker: no real stimulus enters the mesh.

### Implementation
- **OpenClaw → Mesh bridge**: Gateway plugin that POSTs conversation summaries to `http://127.0.0.1:9020/api/mesh/stimulus`
- **Cron-driven stimuli**: Periodic market data, weather, news → relevant organs
- **Heartbeat stimuli**: System health metrics → meta organ
- **Event-driven stimuli**: Email arrival, calendar event → mind organ

> Without wiring OpenClaw events → mesh stimuli, the brain will remain in permanent DeepBasin hibernation regardless of how many organs we add.

---

## Turbulence Regulation Improvement

### Current: Binary 0.1 threshold — causes oscillation
### Proposed: Hysteresis band (0.08-0.12) with regime-specific behaviors

| Turbulence | Regime | Behavior |
|-----------|--------|----------|
| <0.05 | Deep Sleep | Minimal processing, consolidation |
| 0.05-0.1 | Stable Focus | Analytical, precise |
| 0.1-0.3 | Creative Exploration | High connectivity, novel associations |
| 0.3-0.5 | Chaotic Breakthrough | Rapid attractor exploration |
| >0.5 | Emergency | Cool down, stabilize |

### Additional Proposals
- **Organ-specific turbulence targets**: Different organs function best at different levels
- **Attractor-based regulation**: Steer toward known-good attractors rather than generic heat/cool
- **Turbulence cascade matrix**: 6×6 weight matrix: `new_turb[j] = Σ(cascade[i][j] * turb[i])`

---

## Attractor Seeding

Current: Only 1 attractor discovered ("Chaos Initial", att_000). Target: 3-5+ stable attractors.

### Proposed Attractors
1. **DeepFocus** — Low turbulence (0.02-0.05), high engineer activation. Deep coding/analysis.
2. **CreativeStorm** — High turbulence (0.15-0.25), high creative activation. Ideation.
3. **CautiousAnalysis** — Low-medium turbulence (0.05-0.08), high science + crypto activation. Risk assessment.
4. **SocialResonance** — Medium turbulence (0.08-0.12), high mind + meta activation. Communication.
5. **ReactiveGuard** — Spiking turbulence (0.20+ transient), high reflex activation. Anomaly response.

### Implementation
- Add attractor discovery with k-means on trajectory history
- Periodic "dream" mode where nodes explore random trajectories
- Global attractor field maintained by orchestrator

---

## Synaptic Plasticity (Hebbian Learning)

Currently, brain nodes communicate via static HTTP — no learning between connections.

### Proposed
```rust
struct SynapticWeight {
    pre: OrganId,
    post: OrganId,
    strength: f64,    // Modified by: dW = η * pre_act * post_act
    last_strengthened: u64,
}
```

- **Hebbian learning**: Frequently co-activated nodes strengthen their connection
- **Long-term potentiation (LTP)**: Repeated activation patterns get faster routing
- **Long-term depression (LTD)**: Unused connections weaken, eventually prune
- **Weight matrix**: 6×6 in RocksDB, updated per interaction

---

## Performance Optimization Proposals

| Optimization | Impact | Effort | Priority |
|-------------|--------|--------|----------|
| Node binary unification | -55M RAM | Medium | P2 |
| RocksDB shared instance | -disk seeks | Medium | P3 |
| Connection pooling (HTTP/2) | -40% latency | Low | P1 |
| Lock-free state (DashMap) | Eliminate contention | Low | P1 |
| Zero-copy deserialization | -15-20% parsing | Low | P1 |
| Batch stimuli processing | 10x throughput in idle | Medium | P2 |
| Binary inter-node protocol | 3-5x serialization | Medium | P3 |
| SIMD vector search | 4x memory/search | High | P4 |
| Shared-memory channels (crossbeam) | Sub-ms latency | Medium | P3 |

---

## Security Hardening

| Action | Priority | Notes |
|--------|----------|-------|
| Fix Gateway WS 1006 | P0 | Blocks cron/tasks |
| Disable allowInsecureAuth | P0 | Active security risk |
| Mesh API port 9020 /health | P1 | Returns 404, fix or document |
| Organ authentication (HMAC) | P2 | Prevent unauthorized stimuli |
| Organ TLS | P3 | HTTP only currently |
| Rate limiting on reflex organ | P2 | Prevent reflex amplification loops |
| Audit logging for organ state | P2 | Traceability |
| Gateway auth token rotation | P1 | Unchanged since setup |

---

## Python → Rust Migration Tracker

| Process | RAM | Priority | Target Crate |
|---------|-----|----------|-------------|
| decision_engine | ~11M | P1 | soullink-decision |
| market_injector | ~36M | P2 | soullink-market |
| reinforcement_critic | ~23M | P2 | soullink-critic |
| sl13-mod-evolve | ~17M | P3 | Already has Rust binary |

**Note:** `night-cycle-engine` Rust binary already exists at `/mnt/nvme/soullink_brain/openevolve-rust/target/release/night-cycle-engine`. `sl13-mod-evolve.py` should be killed and replaced immediately.

---

## Implementation Priority Order (Consensus Across Reports)

1. **Memory organ** — Enables persistent learning (★★★★★ emergence)
2. **Reflex organ** — Enables fast reactive responses (★★★★ emergence)
3. **Synthesis organ** — Relieves meta bottleneck (★★★★★ emergence)
4. **Stimulus pipeline** — Wire OpenClaw events to mesh (CRITICAL blocker)
5. **Decision engine** — Port Python → Rust (simplest, ~11M RAM)
6. **Kill sl13-mod-evolve.py** — Replace with existing Rust binary
7. **soullink-server-core** — Shared library for boilerplate elimination
8. **Attractor seeding** — Add 4-5 predefined attractors
9. **Turbulence hysteresis** — Replace binary threshold with band
10. **Connection pooling** — Persistent HTTP/2 between nodes