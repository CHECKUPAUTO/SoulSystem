# soullink-foresight — Anticipation / Prediction Organ

**Port:** 9040  
**Type:** Anticipation / Prédiction  
**Emergence Score:** HIGH  
**Status:** Proposal (not yet implemented)  
**Source:** night_cycle_20260414_1402.md  
**Created:** 2026-04-14

## Raison d'être

Prédire les besoins futurs basé sur les patterns historiques. Activer les ressources avant la demande (prefetch cognitif).

## Architecture

```
soullink-foresight/
├── src/main.rs          — Axum HTTP server, port 9040
├── src/predictor.rs     — Time-series prediction engine (exponential smoothing + neural hints)
├── src/pattern_db.rs    — RocksDB pattern storage (historical sequences)
├── src/prefetch.rs      — Resource prefetch coordinator
├── src/temporal.rs      — Temporal pattern extraction (circadian, weekly, seasonal)
└── Cargo.toml          — axum, tokio, rocksdb, chrono, serde
```

## API Interfaces

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/foresight/predict` | Prédiction pour un contexte donné |
| GET | `/api/foresight/patterns` | Patterns temporels identifiés |
| POST | `/api/foresight/prefetch` | Déclencher prefetch de ressources |
| GET | `/api/foresight/health` | Health check |

## Neural Configuration

- **Neurons:** 400
- **Default Attractor:** StableOrbit
- **Emergence:** HIGH — permet à l'Integration de passer de réactif à proactif

## Priority

Ranked #2 by emergence (after creativity). Highest ROI for proactive behavior.

## Dependencies

- soullink-memory (port 9030) — historical pattern storage
- soullink-integration (port 9032) — coordination of prefetch signals
- soullink-orchestrator (port 9020) — mesh registration