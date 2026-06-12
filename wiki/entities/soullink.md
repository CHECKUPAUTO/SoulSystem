# SoulLink

_Entity page — the neural mesh system._

## Overview
- **Type**: Persistent neural mesh (6 Rust nodes + orchestrator)
- **Architecture**: Rust native (axum + tokio + dashmap)
- **Orchestrator**: Port 9020 (Rust v3)
- **Storage**: /mnt/nvme/soullink_brain/

## Nodes
| Node | Port | Status |
|------|------|--------|
| Science | 9010 | Online |
| Mind | 9011 | Online |
| Engineer | 9012 | Online |
| Crypto | 9013 | Online |
| Creative | 9014 | Online |
| Meta | 9015 | Online |

## Attractors
- **DeepBasin**: Calm, receptive state
- **StableOrbit**: Predictable recurrent patterns
- **StrangeAttractor**: Chaotic emergent creativity
- **Transient**: Transition between regimes

## History
- V5 → V6 Immortal (2026-04-05): Docker removed, systemd native
- V6 → V11 GPU Stable (2026-04-09): Session pruning, Dual-Lock
- V11 → V12 (2026-04-10): LIF vectorized NumPy + TurbulenceEngine SIMD
- V12 → V13 Rust Only (2026-04-12): All Python archived, 6× soullink-node

## Services
- `sl13-brain-*` (6 systemd services per node)
- Legacy disabled: brain-v9, brain-v11, brain-cortex, brain-crypto-rocksdb

## See Also
- [rust-migration](../concepts/rust-migration.md)