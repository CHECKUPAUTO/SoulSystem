# soullink-homeostasis — Autorégulation Systémique Organ

**Port:** 9041 | **Type:** Autorégulation systémique | **Neurons:** 200 | **Default Attractor:** DeepBasin

**Source:** OpenEvolve Night Cycle 131 (2026-04-14 14:02)

## Raison d'être

Maintenir l'équilibre global du mesh. Monitorer charge, latence, cohérence. Déclencher throttling ou scaling. Transforme le mesh en système auto-régulé.

## Architecture

```
├── src/main.rs          — Axum HTTP server, port 9041
├── src/regulator.rs     — PID controller for mesh homeostasis
├── src/vitals.rs        — System vitals collection (CPU, mem, latency, queue depth)
├── src/thermostat.rs    — Thermal/charge management (turbulence → throttling)
├── src/recovery.rs      — Self-healing: restart stale organs, rebalance load
└── Cargo.toml          — axum, tokio, sysinfo, serde, tracing
```

## API Interfaces

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/homeostasis/vitals` | GET | Current system vitals |
| `/api/homeostasis/regulate` | POST | Trigger regulation cycle |
| `/api/homeostasis/thresholds` | GET | Current regulation thresholds |
| `/api/homeostasis/recover` | POST | Trigger self-healing |

## Emergence Score: MEDIUM-HIGH

Auto-régulation, principalement stabilisation plutôt qu'émergence créative.

## Dependencies

- sysinfo crate (system metrics)
- Links to: All organs (monitoring), Orchestrator (9020)

## Build Estimate

3-5 hours (Rust, Axum, sysinfo integration)

## Related Proposals

- Dynamic Neuron Allocation — allocation basée sur charge (200-800 range), via Homeostasis organ
- Rate limiting per-organ — prévenir cascade failure