# Neural Mesh Health Snapshot (2026-04-13 22:54)

> Extracted from night_cycle_20260413_2254.md

## Mesh Status

- **Online nodes**: 6/6
- **Total neurons**: 2400
- **Total brains**: 6
- **Turbulence**: 0.0939 (just below 0.1 instability threshold)
- **Current Attractor**: "Chaos Initial" (att_000, stable, 990 visits)
- **Field Mean Activation**: 0.311
- **Regulation State**: `excited` — last command: `heat` intensity 1.05 on all nodes

## Node Health Detail

| Node | Port | Neurons | Attractor | Hz | Pressure | Activation | Health |
|------|------|---------|-----------|-----|----------|------------|--------|
| Science | 9010 | 400 | DeepBasin | 0.0 | 0.362 | 0.386 | ✅ Good |
| Mind | 9011 | 400 | DeepBasin | 0.0 | 0.443 | 0.301 | ✅ Good |
| Engineer | 9012 | 400 | DeepBasin | 0.0 | 0.272 | 0.347 | ✅ Good |
| Crypto | 9013 | 400 | DeepBasin | 0.0 | 0.348 | 0.271 | ✅ Good |
| Creative | 9014 | 400 | DeepBasin | 0.0 | 0.322 | 0.295 | ✅ Good |
| Meta | 9015 | 400 | DeepBasin | 0.0 | 0.463 | 0.266 | ⚠️ High pressure, low activation |

## Key Observations

1. **All nodes in DeepBasin** — system is calm but dormant (hz=0.0 across all)
2. **Regulation is "excited"** with heat command on all nodes — system trying to increase activity
3. **Turbulence at 0.094** — on the cusp of instability threshold (0.1)
4. **Meta node concern**: highest pressure (0.463) but lowest activation (0.266) — potential bottleneck in self-reflection
5. **Science highest activation** (0.386) — correctly aligned with information processing role
6. **Pressure ordering**: meta > mind > science > crypto > creative > engineer
7. **Activation ordering**: science > engineer > mind > creative > crypto > meta

## Recommended Actions

1. **Turbulence injection**: Add scheduled pulses (every 5min) to prevent neural stagnation
2. **Meta node relief**: Implement INTEGRATION organ (port 9018) to offload cross-node synthesis from Meta
3. **Attractor diversity**: Different organs should have different default attractors
4. **Pressure-driven routing**: High-pressure nodes should delegate to lower-pressure neighbors

## Running Services (12 active)

| Service | Status | Details |
|---------|--------|---------|
| `sl-node-science` | ✅ active | Port 9010, v6.1, Rust native |
| `sl-node-mind` | ✅ active | Port 9011, v6.1, Rust native |
| `sl-node-engineer` | ✅ active | Port 9012, v6.1, Rust native |
| `sl-node-crypto` | ✅ active | Port 9013, v6.1, Rust native |
| `sl-node-creative` | ✅ active | Port 9014, v6.1, Rust native |
| `sl-node-meta` | ✅ active | Port 9015, v6.1, Rust native |
| `sl-node-logic` | ✅ active | Mesh orchestrator, 64-core architect |
| `sl13-mod-decision_engine` | ✅ active | Decision engine module |
| `sl13-mod-evolve` | ✅ active | Self-learning engine (V14) |
| `sl13-mod-market_injector` | ✅ active | Market data injection |
| `sl13-mod-reinforcement_critic` | ✅ active | Reinforcement learning critic |
| `soullink-orchestrator` | ✅ active | Neural Mesh Orchestrator v3 (Rust) |

## Source

- `night_cycle_20260413_2254.md`

## Last Updated

2026-04-13T23:11:00+02:00 — Auto-apply cycle