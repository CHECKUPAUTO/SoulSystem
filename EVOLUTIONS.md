# SoulSystem Evolution Roadmap 🦞

## 1. Architecture & Performance ✅
- **Zero-Copy IPC**: ✅ `soullink-shm` — memfd + mmap + UDS fd-passing, ShmBus (16-slot broadcast)
- **Dynamic VRAM Management**: ✅ `soullink-vram` — 5 priority levels, 4 pressure levels, reference counting
- **Distributed Mesh**: ✅ `soullink-registry` — service directory, serialize/merge for multi-node sync

## 2. Autonomy & AI ✅
- **Automated Fine-Tuning Pipeline**: ✅ `soullink-trainer` — trajectory recorder, filter, DPO pair export
- **Hierarchical Memory**: ✅ `soullink-memory-hierarchy` — working (ring buffer) → episodic (decay) → semantic (consolidation)
- **Mixture of Experts (MoE)**: ✅ `soullink-moe` — task classifier + expert router by domain/load

## 3. New Tools ✅
- **`soul-top`**: ✅ Ratatui TUI — organ health, turbulence, events, gauges
- **`soul-chaos`**: ✅ Chaos Monkey — Latency, Error, Corrupt, Kill, Flood injection
- **`soul-shell`**: ✅ Interactive CLI — status, inject, memory, health commands

## 4. Maintenance & Reliability ✅
- **Crate Unification**: ✅ Bus, circuit breaker, soul-memory unified into common library
- **Advanced Autocode**: Partially done (AutoCoder exists, metacognition-driven refactoring pending)
- **Dependency Hygiene**: ✅ 35 crates migrated to workspace deps, openevolve unified
- **Dead Code Cleanup**: ✅ 3 items deleted, 3 annotations removed
