# Neural Mesh Architecture & Stimulus Pipeline Design

**Source:** Night cycle reports 2026-04-14 (00:00 through 05:01)
**Created:** 2026-04-14 by auto-apply
**Status:** Reference documentation (no code changes)

---

## Current State: Structurally Alive, Functionally Dormant

- **All 6 nodes**: DeepBasin (or Chaos Initial), Hz = 0.0
- **Orchestrator queries**: 0 (zero external stimuli processed)
- **Meta organ**: Pressure 0.463, activation 0.266 (severe bottleneck)
- **Discovered attractors**: 1 (Chaos Initial, att_000)
- **Turbulence**: 0.094 (near 0.1 threshold, oscillating)

**Root cause:** The mesh receives ZERO external input. Without the stimulus pipeline, the brain stays in permanent hibernation regardless of organ count.

---

## Stimulus Pipeline Design

### Architecture
```
OpenClaw Gateway
    │
    ├── Conversation events ──→ POST /api/mesh/stimulus {type: "conversation", ...}
    ├── Cron events ──────────→ POST /api/mesh/stimulus {type: "cron", ...}
    ├── Email arrival ────────→ POST /api/mesh/stimulus {type: "email", ...}
    ├── Calendar events ──────→ POST /api/mesh/stimulus {type: "calendar", ...}
    └── Health metrics ───────→ POST /api/mesh/stimulus {type: "health", ...}
         │
         ▼
    Orchestrator (port 9020)
         │
    ┌────┼────┬────┬────┬────┬────┐
    Science Mind Engin Crypto Creat Meta
    9010  9011 9012 9013  9014  9015
```

### Implementation Path
1. **Gateway plugin** that hooks into OpenClaw conversation lifecycle
2. **Stimulus format**: `{ type, content, priority, source, timestamp }`
3. **Orchestrator routing**: Routes stimulus to relevant organs based on type
4. **Turbulence response**: Each stimulus perturbs the mesh, causing attractor transitions

### Priority Routing
| Stimulus Type | Primary Target | Secondary |
|--------------|----------------|-----------|
| Conversation | Mind | Meta, Creative |
| Code/architecture | Engineer | Science |
| Market data | Crypto | Engineer |
| Research query | Science | Mind |
| Health/system | Meta | — |
| Email/message | Mind | — |
| Calendar | Mind | Meta |

---

## Cross-Node Resonance

### Current: Static HTTP communication
### Proposed: Dynamic resonance with Hebbian learning

### Resonance Protocol
When two organs' activation patterns correlate >0.7:
- **Resonant pairs** boost each other's processing (positive feedback)
- **Anti-resonant pairs** dampen (negative feedback → stability)
- Creates emergent specialization without explicit programming

### Synaptic Weight Matrix
```rust
struct SynapticWeight {
    pre: OrganId,
    post: OrganId,
    strength: f64,    // Modified by: dW = η * pre_act * post_act
    last_strengthened: u64,
}
```

6×6 matrix stored in RocksDB, updated on every cross-node interaction.

### Phase Locking
When two organs enter co-active states, their processing cycles synchronize:
```rust
struct ResonanceMatrix {
    pairs: HashMap<(OrganId, OrganId), PhaseOffset>,
    synchronization_threshold: f64,
}
```

---

## Turbulence Cascade

### Current: Per-node turbulence, no cross-talk
### Proposed: Cross-node influence with dampening

```
new_turb[j] = Σ(cascade[i][j] * turb[i])
```

Example cascade matrix:
```
         → Science  Mind  Engineer  Crypto  Creative  Meta
Science  │  1.0    0.2    0.3      0.1      0.15     0.1  │
Mind     │  0.1    1.0    0.1      0.1      0.2      0.3  │
Engineer │  0.2    0.1    1.0      0.05     0.1      0.1  │
Crypto   │  0.05   0.3    0.2      1.0      0.1      0.15 │
Creative │  0.15   0.2    0.1      0.1      1.0      0.2  │
Meta     │  0.1    0.1    0.1      0.1      0.1      1.0  │
```

Key properties:
- High crypto turbulence → modulates engineer (caution), creative (opportunity)
- Meta can dampen global turbulence during critical operations
- Creative turbulence amplifies science (novel connections)

### Turbulence-Modulated Routing
| Turbulence | Route To | Processing Mode |
|-----------|----------|----------------|
| High (>0.1) | Creative + Meta | Creative exploration |
| Low (<0.1) | Science + Engineer | Analytical precision |
| Emergency (>0.5) | Reflex | Fast response |

---

## Sleep Consolidation

When all organs are in DeepBasin for >30 minutes:
1. Trigger "sleep cycle"
2. Integration organ replays the day's stimuli
3. Strengthen important pathways (Hebbian potentiation)
4. Prune weak connections (long-term depression)
5. Consolidate short-term memories → long-term storage

---

## Performance: Node Binary Unification

### Current: 6 separate processes, ~9M each = ~55M total
### Proposed: Single launcher managing all 6 as threads

Benefits:
- ~55M RAM savings (shared code pages, single runtime)
- Sub-microsecond inter-node communication (vs 1-5ms HTTP)
- Simpler deployment (single binary)
- Better CPU cache utilization

Implementation:
```rust
// soullink-launcher/src/main.rs
fn main() {
    let config = vec![
        NodeConfig { name: "science", port: 9010, neurons: 400 },
        NodeConfig { name: "mind", port: 9011, neurons: 400 },
        // ... etc
    ];
    let handles: Vec<JoinHandle<()>> = config.into_iter()
        .map(|c| tokio::spawn(run_node(c)))
        .collect();
    // await all
}
```

---

## Gateway WS 1006 Fix Analysis

**Root cause (across all reports):** Environment variable `OPENCLAW_GATEWAY_URL` points to port 18889, but gateway listens on 18890. This config mismatch causes WebSocket 1006 abnormal closure.

**Impact:** Affects `openclaw cron list`, task management API calls, and remote access.

**Fix options:**
1. Update env var: `OPENCLAW_GATEWAY_URL=ws://127.0.0.1:18890/ws`
2. Update gateway config to listen on 18889
3. Restart gateway: `openclaw gateway restart`

> ⚠️ Requires human approval (gateway configuration change)