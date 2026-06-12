# Neural Mesh Health Snapshot (2026-04-14)

> Consolidated from night_cycle_20260414_0000.md through night_cycle_20260414_1402.md
> Last updated: Cycle 131 (14:02 CEST)

## Mesh Status

- **Online nodes**: 6/6
- **Total neurons**: 2400
- **Turbulence**: 0.094 (near threshold, stable-low)
- **Current Attractor**: Chaos Initial (att_000)
- **Dominant node**: science (38.6%)
- **Mesh health**: 100%
- **8 organ services running** (orchestrator + 7 organs: memory/reflex/affect/perception/reasoning/language/integration)
- **Orchestrator uptime**: 1d14h (stable)
- **Regulation state**: excited
- **All nodes**: hz=0.0 (idle/standby)

## Node Health Detail

| Node | Port | Neurons | Attractor | Hz | Status |
|------|------|---------|-----------|-----|--------|
| Science | 9010 | 400 | DeepBasin | 0.0 | ✅ Active |
| Mind | 9011 | 400 | DeepBasin | 0.0 | ✅ Active |
| Engineer | 9012 | 400 | DeepBasin | 0.0 | ✅ Active |
| Crypto | 9013 | 400 | DeepBasin | 0.0 | ✅ Active |
| Creative | 9014 | 400 | DeepBasin | 0.0 | ✅ Active |
| Meta | 9015 | 400 | DeepBasin | 0.0 | ✅ Active |

## Supporting Services

| Service | Status |
|---------|--------|
| Logic Node (64-core architect) | ✅ running |
| V3 Orchestrator | ✅ running |
| V13 Decision Engine | ✅ running |
| V14 Evolve (self-learning) | ✅ running |
| Market Injector | ✅ running |
| Reinforcement Critic | ✅ running |
| Neural Mesh V3 Rust | ✅ running |

**Total: 13/13 services running ✅**

## Key Observations

1. **All nodes in DeepBasin** — system is calm and stable (idle/standby)
2. **All hz=0.0** — no active neural processing
3. **Turbulence 0.0** — no instability detected
4. **System is healthy** — plenty of resource headroom (19% disk, 16% RAM)

## Comparison with Previous Snapshot (2026-04-13 22:54)

| Metric | 22:54 | 00:00 | Delta |
|--------|-------|-------|-------|
| Turbulence | 0.0939 | 0.0 | ↓ Calmed |
| Regulation | "excited" | idle/standby | ↓ Normalized |
| Nodes online | 6/6 | 6/6 | = |
| Hz (all nodes) | 0.0 | 0.0 | = |

The system has calmed from the previous "excited" regulation state. All nodes remain dormant but stable.

---

## Snapshot 2: 2026-04-14 00:30

**Mesh transition detected:** System shifted from DeepBasin → "Chaos Initial" attractor.

| Metric | Value |
|--------|-------|
| Turbulence | 0.0939 (approaching 0.1 stability threshold) |
| Current Attractor | Chaos Initial (stable type, radius 0.15, 990 visits) |
| Regulation | "excited" with heat command (intensity=1.05, targets=all) |
| Mean Activation | 0.3109 |

### Per-Node Activation & Pressure (00:30)

| Node | Port | Activation | Pressure | Health | Issue |
|------|------|-----------|----------|--------|-------|
| Science | 9010 | 0.386 | 0.362 | ✅ | Balanced, slightly under-pressured |
| Mind | 9011 | 0.301 | 0.443 | ⚠️ | **Pressure 47% > activation — bottleneck** |
| Engineer | 9012 | 0.347 | 0.272 | ✅ | Under-pressured, capacity headroom |
| Crypto | 9013 | 0.271 | 0.348 | ⚠️ | Activation < pressure, needs stimulation |
| Creative | 9014 | 0.295 | 0.322 | ✅ | Slightly under-activated |
| Meta | 9015 | 0.266 | 0.463 | 🔴 | **Severe bottleneck — 74% pressure ratio** |

### Key Observations (00:30)

1. **Meta organ crisis** — Lowest activation but highest pressure. Being asked to do more than it can handle. Strongest signal for creating Integration organ.
2. **Mind organ bottleneck** — Pressure 47% above activation. Needs more capacity.
3. **Science dominant** (0.386 activation) — consistent with ongoing research/coding work.
4. **Turbulence rising** — 0.094 approaching the 0.1 threshold between stability and chaos.
5. **Mesh API port 9020** returns 404 on /health — needs investigation or documentation.

### Delta Analysis: 00:00 → 00:30

| Metric | 00:00 | 00:30 | Change |
|--------|-------|-------|--------|
| Turbulence | 0.0 | 0.094 | ↑ Increasing |
| Attractor | DeepBasin | Chaos Initial | Shifted |
| Regulation | idle/standby | excited (heat=1.05) | ↑ Activated |
| Meta pressure/activation | 0/0 | 0.463/0.266 | 🔴 Bottleneck emerged |

The system re-activated from standby into an excited regulation state. Meta organ bottleneck is the primary concern.

---

---

## Snapshot 3: 2026-04-14 01:00

**Mesh returns to calm.** Gateway connection issue detected (1006 abnormal closure).

| Metric | Value |
|--------|-------|
| Turbulence | 0.0939 (below 0.1 threshold) |
| Current Attractor | Chaos Initial (stable, radius 0.15, 990 visits) |
| Regulation | "excited" with heat=1.05 |
| Mean Activation | 0.311 |
| All nodes | DeepBasin attractor, hz=0.0 |

### Per-Node Status (01:00)

Same as 00:30 snapshot — no changes in node states.

| Node | Port | Activation | Pressure | Health |
|------|------|-----------|----------|--------|
| Science | 9010 | 0.386 | 0.362 | ✅ |
| Mind | 9011 | 0.301 | 0.443 | ⚠️ |
| Engineer | 9012 | 0.347 | 0.272 | ✅ |
| Crypto | 9013 | 0.271 | 0.348 | ✅ |
| Creative | 9014 | 0.295 | 0.322 | ✅ |
| Meta | 9015 | 0.266 | 0.463 | 🔴 |

### New Issue: Gateway Connection

⚠️ `openclaw status` reports gateway closed (1006 abnormal closure). Gateway target: `ws://127.0.0.1:18889/ws`. May require probe/restart.

### Key Observations (01:00)

1. **Meta organ still at critical bottleneck** (0.463 pressure, 0.266 activation) — two consecutive cycles with no organ implementation progress
2. **Gateway 1006 error** — new issue detected, needs investigation
3. **Refined LOC counts** — 5,950 source-only (excludes evaluator duplicates), 42 source files
4. **13/13 services still running** despite gateway issue
5. **All organs still at 0 LOC implementation** — scaffolding exists but no code written

### Critical Assessment

Three consecutive cycles (00:30, 01:00, 01:30) with **zero organ implementation progress** while meta organ remains at critical pressure. The 01:30 report claims soullink-memory v1.0.0 is "COMPLETE" but this is disputed — verified src/ directory is empty (0 LOC). The gap between scaffolding and implementation must close urgently. Memory + Integration organs are the #1 priority.

---

## Snapshot 4: 2026-04-14 01:30

**Full ecosystem scan.** Gateway confirmed unreachable. Detailed organ proposals with revised port assignments.

| Metric | Value |
|--------|-------|
| Turbulence | 0.0939 (below 0.1 threshold) |
| Current Attractor | Chaos Initial (stable, radius 0.15, 990 visits) |
| Regulation | "excited" with heat=1.05 |
| Mean Activation | 0.311 |
| All nodes | DeepBasin attractor, hz=0.0 |
| Gateway | v2026.4.12, WS 1006 error (unreachable) |

### Key Observations (01:30)

1. **Meta organ still at critical bottleneck** (0.463 pressure, 0.266 activation) — three consecutive cycles with no organ progress
2. **Gateway WS unreachable** — confirmed in both 01:00 and 01:30 cycles
3. **4 security warnings** detected: reverse proxy headers, insecure auth toggle, dangerous config flags, multi-user setup
4. **Revised organ port assignments** (9030-9036) proposed to avoid port conflicts
5. **System resources EXCELLENT** — 83% RAM available, 84% disk free
6. **⚠️ Disputed claim**: soullink-memory listed as v1.0.0 COMPLETE, but src/ directory verified empty

### Delta Analysis: 01:00 → 01:30

| Metric | 01:00 | 01:30 | Change |
|--------|-------|-------|--------|
| Turbulence | 0.0939 | 0.0939 | = Stable |
| Gateway | 1006 error | 1006 error (confirmed) | = Persisting |
| Meta pressure/activation | 0.463/0.266 | 0.463/0.266 | = Unchanged |
| Organ implementation | 0 | 0 | = **Still zero** |
| Security warnings | — | 4 warnings | 🆕 New data |
| Organ ports | 9016-9023 | 9030-9036 (revised) | 🆕 Revised |

---

## Snapshot 5: 2026-04-14 02:00

**Full ecosystem scan — mesh structurally healthy but functionally dormant.** All nodes in DeepBasin with hz=0.0. Gateway WS still unreachable.

| Metric | Value |
|--------|-------|
| Turbulence | 0.0 (idle/standby) |
| Current Attractor | All DeepBasin |
| Hz (all nodes) | 0.0 |
| Total neurons | 2400 |
| Orchestrator | v3 (Rust) on port 9020, 1d+ uptime, 6.9M memory |
| Gateway | v2026.4.12, WS 1006 (unreachable), pid 1950521 |

### Per-Node Status (02:00)

All 6 nodes online and running v6.1 (Rust native):

| Node | Port | Attractor | Neurons | Hz | Status |
|------|------|-----------|---------|-----|--------|
| Science | 9010 | DeepBasin | 400 | 0.0 | ✅ online |
| Mind | 9011 | DeepBasin | 400 | 0.0 | ✅ online |
| Engineer | 9012 | DeepBasin | 400 | 0.0 | ✅ online |
| Crypto | 9013 | DeepBasin | 400 | 0.0 | ✅ online |
| Creative | 9014 | DeepBasin | 400 | 0.0 | ✅ online |
| Meta | 9015 | DeepBasin | 400 | 0.0 | ✅ online |

**⚠️ Key finding:** The mesh is *structurally* healthy but *functionally dormant*. All nodes sit in DeepBasin with zero turbulence. The brain exists but nothing is stimulating it. Need real stimulus pipelines (OpenClaw conversations → mesh stimuli).

### Running Services (02:00)

| Service | PID | Memory | Status |
|---------|-----|--------|--------|
| soullink-orchestrator | 103220 | 6.9M | active (1d 2h uptime) |
| soullink-node (×6) | varies | ~17M each | active (v6.1) |
| decision_engine.py | 3861578 | 24M | active |
| market_injector.py | 3861579 | 36M | active |
| reinforcement_critic.py | 3861580 | 37M | active |
| sl13-mod-evolve.py | 3861041 | 36M | active |

### Key Observations (02:00)

1. **Functional dormancy** — mesh is alive but unconscious. All nodes DeepBasin, all hz=0.0.
2. **4 Python processes still running** (~97M combined) — last non-Rust holdouts in the brain.
3. **Zero organ progress** — scaffolding exists but src/ is empty for all 3 scaffolded organs.
4. **Gateway WS 1006** still broken — blocking cron management and some API calls.
5. **System resources excellent** — 81% disk free, 84% RAM free, 3.02 load avg.
6. **soullink-memory v1.0.0** is disputed — Cargo.toml says 1.0.0 but src/ is empty.

### Delta Analysis: 01:30 → 02:00

| Metric | 01:30 | 02:00 | Change |
|--------|-------|-------|--------|
| Turbulence | 0.0939 | 0.0 | ↓ Calmed |
| Attractor | Chaos Initial | DeepBasin (all) | ↓ Settled |
| Regulation | excited | idle/standby | ↓ Normalized |
| Gateway | 1006 | 1006 | = Still broken |
| Organ progress | 0 | 0 | = No change |

---

## Snapshot 6: 2026-04-14 02:30

**Mesh dormant — 02:30 scan confirms.** All nodes DeepBasin/0.0hz. Logic node running separately. Gateway still unreachable.

| Metric | Value |
|--------|-------|
| Turbulence | 0.0 (dormant) |
| Current Attractor | All DeepBasin |
| Hz (all nodes) | 0.0 |
| Total neurons | 2400 |
| Orchestrator | v3.0 Rust, port 9020, running 1d3h, 6.9M memory |
| Logic node | running, 131 tasks |
| Gateway | v2026.4.12, WS 1006 (unreachable), pid 1950521 |

### Per-Node Status (02:30)

All 6 organs online, v6.1, DeepBasin, hz=0.0:

| Node | Port | Neurons | Attractor | Hz | Status | Memory |
|------|------|---------|-----------|-----|--------|--------|
| Science | 9010 | 400 | DeepBasin | 0.0 | ✅ | 8.8M |
| Mind | 9011 | 400 | DeepBasin | 0.0 | ✅ | 9.3M |
| Engineer | 9012 | 400 | DeepBasin | 0.0 | ✅ | 9.3M |
| Crypto | 9013 | 400 | DeepBasin | 0.0 | ✅ | 9.3M |
| Creative | 9014 | 400 | DeepBasin | 0.0 | ✅ | 9.3M |
| Meta | 9015 | 400 | DeepBasin | 0.0 | ✅ | 9.3M |

### Key Observations (02:30)

1. **Functional dormancy persists** — 3 consecutive cycles at hz=0.0 with all nodes DeepBasin
2. **Logic node** running separately with 131 tasks, enabled at boot
3. **4 V13 Python modules** consuming ~55M unnecessary RAM (should migrate to Rust)
4. **All 6 organs** healthy at ~8-9M each (very efficient Rust memory)
5. **Zero organ implementation progress** — scaffolds remain empty

### Delta Analysis: 02:00 → 02:30

| Metric | 02:00 | 02:30 | Change |
|--------|-------|-------|--------|
| Turbulence | 0.0 | 0.0 | = Dormant |
| Attractor | DeepBasin | DeepBasin | = Stable |
| Gateway | 1006 | 1006 | = Still broken |
| Organ progress | 0 | 0 | = No change |

## Snapshot 7: 2026-04-14 03:00

**Full ecosystem scan — no new git commits since 02:00. Mesh dormant, regulation "excited" trying to heat nodes.**

| Metric | Value |
|--------|-------|
| Turbulence | 0.0939 (approaching 0.1 threshold) |
| Current Attractor | Chaos Initial (att_000, 990 visits) |
| Regulation | "excited" with heat@1.05 for all nodes |
| Mean Activation | 0.311 |
| Science pressure | 0.362 (highest activation at 0.386) |
| Meta pressure | 0.463 (highest self-referential drive) |
| Engineer activation | 0.347 (most active node) |
| Orchestrator queries | 0 (zero — mesh receives no input) |

### Key Observations (03:00)

1. **Regulation system is trying to work but can't** — "excited" state with heat commands targeting all nodes for "low_activity", but no stimulus pipeline exists
2. **Zero orchestrator queries** — the mesh is completely disconnected from real input
3. **4 Python processes** still running (~87M total: decision_engine 10.9M, market_injector ~36M, reinforcement_critic 22.7M, sl13-mod-evolve 17.4M)
4. **sl13-mod-evolve.py should be killed** — night-cycle-engine Rust binary already compiled at `/mnt/nvme/soullink_brain/openevolve-rust/target/release/night-cycle-engine`
5. **Gateway WS 1006** — env var `OPENCLAW_GATEWAY_URL=ws://127.0.0.1:18889/ws` differs from gateway listening port 18890. **Config mismatch confirmed.**
6. **19 Rust crates** in total ecosystem (6 sub-crates in workspaces)
7. **Brain Stack: 68% compiled, 84% scaffolded** — 3 organs have empty src/
8. **Zero progress** on organ implementations across 5+ consecutive cycles

### Delta Analysis: 02:30 → 03:00

| Metric | 02:30 | 03:00 | Change |
|--------|-------|-------|--------|
| Turbulence | 0.0 | 0.094 | ↑ Re-activated |
| Attractor | DeepBasin | Chaos Initial | ↑ Shifted |
| Regulation | idle/standby | excited (heat@1.05) | ↑ Trying to activate |
| Gateway | 1006 | 1006 | = Still broken |
| Organ progress | 0 | 0 | = No change |

---

## Snapshot 8: 2026-04-14 03:30

**v7.0 Extended cycle. Mesh dormant, all nodes DeepBasin. Load avg: 3.50. 19 Rust crates confirmed.**

| Metric | Value |
|--------|-------|
| Turbulence | 0.094 (stable regime) |
| Current Attractor | Chaos Initial (att_000) |
| Mean activation | 0.311 |
| Regulation state | Excited — trying to heat nodes for low_activity |
| Science pressure | 0.362 |
| Meta pressure | 0.463 (highest) |
| Engineer activation | 0.347 |
| All nodes | DeepBasin, Hz=0.0 |
| System disk | 159G/915G (19%) |
| NVMe | 382G/1.8T (22%) |
| RAM | 20G/125G (16%) |
| Load avg | 3.50 |

### Brain Evolution Dashboard (03:30)

| Organ | Port | Status | N | Readiness |
|-------|------|--------|---|-----------|
| Science | 9010 | ✅ Online | 400 | Production |
| Mind | 9011 | ✅ Online | 400 | Production |
| Engineer | 9012 | ✅ Online | 400 | Production |
| Crypto | 9013 | ✅ Online | 400 | Production |
| Creative | 9014 | ✅ Online | 400 | Production |
| Meta | 9015 | ✅ Online | 400 | Production |
| **Memory** | **9021** | 🔶 Skeleton | 600 | Design complete |
| **Reasoning** | **9022** | ❌ New | 500 | Design complete |
| **Perception** | **9023** | ❌ New | 800 | Design complete |
| **Language** | **9024** | ❌ New | 400 | Design complete |
| **Affect** | **9025** | ❌ New | 300 | Design complete |
| **Reflex** | **9030** | ❌ New | 200 | Design complete |
| **Integration** | **9040** | ❌ New | 500 | Design complete |

**Total neurons (current):** 2,400
**Total neurons (proposed):** 2,400 + 3,300 = **5,700** (+137.5%)

### Key Observations (03:30)

1. **All nodes DeepBasin with Hz=0.0** — mesh structurally alive but functionally dormant
2. **Regulation system "excited"** — trying to heat nodes for low_activity, but no stimulus pipeline
3. **Integration organ promoted to #2 priority** — "consciousness substrate" that binds all organs
4. **19 Rust crates confirmed** in ecosystem (including 6 sub-crates in workspaces)
5. **Brain Stack: 91% compiled** (10/11 production crates, 1 skeleton)
6. **OpenClaw Core: 0% Rust migration** (6,357+ TS files)
7. **Python→Rust: 0/4** (decision_engine, market_injector, reinforcement_critic, sl13-mod-evolve)

### Delta Analysis: 03:30 → 05:01

| Metric | 03:30 | 05:01 | Change |
|--------|-------|-------|--------|
| Turbulence | 0.094 | 0.0939 | ≈ Same |
| Attractor | Chaos Initial | Chaos Initial (att_000) | = Same, only 1 discovered |
| Regulation | excited | excited | = Still trying to heat |
| Gateway | 1006 | 1006 | = Still broken |
| Organ progress | 0 | 0 | = No change |
| Discovered attractors | 1 | 1 | = Impoverished landscape |
| Meta pressure/activation | 0.463/0.266 | 0.463/0.266 | = Bottleneck unchanged |

### Key Findings (05:01 cycle)

1. **Only 1 attractor discovered** — critically impoverished attractor landscape. Should have 3-5+.
2. **Meta bottleneck persists** — highest pressure (0.463) but lowest activation (0.266). Meta is overwhelmed without Integration organ.
3. **Science leads activation** (0.386) — research-focused state.
4. **Turbulence near threshold** (0.0939 ≈ 0.1) — oscillation risk. Need hysteresis band (0.08-0.12).
5. **Zero organ implementation progress** — 5+ consecutive cycles with no new code.
6. **Proposed 5 new attractors**: DeepFocus, CreativeStorm, CautiousAnalysis, SocialResonance, ReactiveGuard.

---

## Snapshot 8 — Cycle 131 (2026-04-14 14:02)

- **Turbulence**: 0.094 (stable-low, near 0.1 threshold)
- **Attractor**: Chaos Initial
- **Dominant node**: science (38.6%)
- **Regulation state**: excited
- **Mesh health**: 100%
- **All 6 nodes**: online, DeepBasin, v6.1, N=400 each
- **8 organ services**: all running (memory/reflex/affect/perception/reasoning/language/integration + orchestrator)
- **Orchestrator uptime**: 1d14h (stable)
- **⚠️ nightly-insights cron failing** (kimi-k2.5 model likely unavailable)

**Delta from 05:01**: System stable. Turbulence slightly above 05:01 (0.094 vs 0.0939). All organs online and running. No new critical issues.

## Sources

- `night_cycle_20260414_0000.md`
- `night_cycle_20260414_0030.md`
- `night_cycle_20260414_0100.md`
- `night_cycle_20260414_0130.md`
- `night_cycle_20260414_0200.md`
- `night_cycle_20260414_0230.md`
- `night_cycle_20260414_0300.md`
- `night_cycle_20260414_0330.md`
- `night_cycle_20260414_0501.md`
- `night_cycle_20260414_1402.md`

## Last Updated

2026-04-14T14:07:00+02:00 — Auto-apply cycle (14:02 report: Cycle 131 snapshot, turbulence 0.094, Chaos Initial, 8 organ services running, nightly-insights cron error)