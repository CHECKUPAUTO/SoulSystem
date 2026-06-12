# soullink-creativity — Creative Emergence Organ

**Port:** 9042  
**Type:** Génération créative émergente  
**Emergence Score:** VERY HIGH  
**Status:** Proposal (not yet implemented)  
**Source:** night_cycle_20260414_1402.md  
**Created:** 2026-04-14

## Raison d'être

Combinaison conceptuelle, métaphores croisées, pensée latérale. Complément au nœud "creative" (9014) qui est spécialisé.

## Architecture

```
soullink-creativity/
├── src/main.rs          — Axum HTTP server, port 9042
├── src/combinator.rs    — Conceptual combination engine (cross-domain blending)
├── src/metaphor.rs      — Metaphor generation from concept graphs
├── src/divergence.rs    — Divergent thinking module (random walks in concept space)
├── src/convergence.rs   — Convergent evaluation (fitness scoring)
├── src/concept_db.rs    — RocksDB concept graph storage
└── Cargo.toml          — axum, tokio, rocksdb, rand, serde
```

## API Interfaces

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/creativity/combine` | Combine concepts across domains |
| POST | `/api/creativity/metaphor` | Generate metaphor for concept pair |
| POST | `/api/creativity/diverge` | Divergent exploration from seed |
| POST | `/api/creativity/evaluate` | Score creative output fitness |

## Neural Configuration

- **Neurons:** 600
- **Default Attractor:** StrangeAttractor
- **Emergence:** VERY HIGH — StrangeAttractor produit les outputs les plus imprévisibles

## Priority

Ranked #1 by emergence. Highest creative potential of all proposed organs.

## Dependencies

- soullink-memory (port 9030) — concept graph persistence
- soullink-integration (port 9032) — cross-organ coordination
- soullink-affect (port 9034) — emotional modulation of creative output
- creative brain node (port 9014) — specialized creative processing