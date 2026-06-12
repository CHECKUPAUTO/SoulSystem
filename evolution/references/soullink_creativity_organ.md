# soullink-creativity — Génération Créative Émergente Organ

**Port:** 9042 | **Type:** Génération créative émergente | **Neurons:** 600 | **Default Attractor:** StrangeAttractor

**Source:** OpenEvolve Night Cycle 131 (2026-04-14 14:02)

## Raison d'être

Combinaison conceptuelle, métaphores croisées, pensée latérale. Complément au nœud "creative" (9014) qui est spécialisé. StrangeAttractor produit les outputs les plus imprévisibles.

## Architecture

```
├── src/main.rs          — Axum HTTP server, port 9042
├── src/combinator.rs    — Conceptual combination engine (cross-domain blending)
├── src/metaphor.rs      — Metaphor generation from concept graphs
├── src/divergence.rs    — Divergent thinking module (random walks in concept space)
├── src/convergence.rs   — Convergent evaluation (fitness scoring)
├── src/concept_db.rs    — RocksDB concept graph storage
└── Cargo.toml          — axum, tokio, rocksdb, rand, serde
```

## API Interfaces

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/creativity/combine` | POST | Combine concepts across domains |
| `/api/creativity/metaphor` | POST | Generate metaphor for concept pair |
| `/api/creativity/diverge` | POST | Divergent exploration from seed |
| `/api/creativity/evaluate` | POST | Score creative output fitness |

## Emergence Score: VERY HIGH

StrangeAttractor + combinaison cross-domain = outputs radicalement nouveaux. Highest emergence potential of all proposed organs.

## Dependencies

- RocksDB (concept graph storage)
- rand crate (stochastic exploration)
- Links to: Creative node (9014), Affect organ (9034/9035), Language organ (9036/9033)

## Build Estimate

6-8 hours (Rust, Axum, RocksDB, stochastic algorithms)

## Neural Model Notes

StrangeAttractor attractor is critical — divergent thinking thrives in chaotic regimes. The convergence module provides fitness scoring to prevent pure randomness.