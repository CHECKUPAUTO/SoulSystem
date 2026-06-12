# soullink-homeostasis — Self-Regulation Organ

**Port:** 9041  
**Type:** Autorégulation systémique  
**Emergence Score:** MEDIUM-HIGH  
**Status:** Proposal (not yet implemented)  
**Source:** night_cycle_20260414_1402.md  
**Created:** 2026-04-14

## Raison d'être

Maintenir l'équilibre global du mesh. Monitorer charge, latence, cohérence. Déclencher throttling ou scaling.

## Architecture

```
soullink-homeostasis/
├── src/main.rs          — Axum HTTP server, port 9041
├── src/regulator.rs     — PID controller for mesh homeostasis
├── src/vitals.rs        — System vitals collection (CPU, mem, latency, queue depth)
├── src/thermostat.rs    — Thermal/charge management (turbulence → throttling)
├── src/recovery.rs      — Self-healing: restart stale organs, rebalance load
└── Cargo.toml          — axum, tokio, sysinfo, serde, tracing
```

## API Interfaces

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/homeostasis/vitals` | Current system vitals |
| POST | `/api/homeostasis/regulate` | Trigger regulation cycle |
| GET | `/api/homeostasis/thresholds` | Current regulation thresholds |
| POST | `/api/homeostasis/recover` | Trigger self-healing |

## Neural Configuration

- **Neurons:** 200
- **Default Attractor:** DeepBasin
- **Emergence:** MEDIUM-HIGH — transforme le mesh en système auto-régulé

## Priority

Ranked #4 by emergence. Critical for system stability but less creative than others.

## Dependencies

- soullink-orchestrator (port 9020) — mesh health monitoring
- All organ services — vitals collection
- System metrics (CPU, RAM, disk) via sysinfo crate