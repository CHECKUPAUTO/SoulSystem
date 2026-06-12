# soullink-validation — Internal Verification & Audit Organ

**Port:** 9044  
**Type:** Vérification et audit interne  
**Emergence Score:** MEDIUM  
**Status:** Proposal (not yet implemented)  
**Source:** night_cycle_20260414_1402.md  
**Created:** 2026-04-14

## Raison d'être

Avant toute sortie externe, valider cohérence, fact-check, sécurité. "Conscience critique" du système.

## Architecture

```
soullink-validation/
├── src/main.rs          — Axum HTTP server, port 9044
├── src/verifier.rs      — Output coherence verification
├── src/fact_check.rs    — Cross-reference validation (link to Perception/Reasoning)
├── src/safety_gate.rs   — Safety boundary enforcement
├── src/audit_log.rs     — RocksDB immutable audit trail
├── src/critic.rs        — Internal critic scoring
└── Cargo.toml          — axum, tokio, rocksdb, serde, sha2
```

## API Interfaces

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/validation/verify` | Verify output coherence |
| POST | `/api/validation/factcheck` | Fact-check claim |
| POST | `/api/validation/safety` | Safety boundary check |
| GET | `/api/validation/audit` | Audit trail query |

## Neural Configuration

- **Neurons:** 200
- **Default Attractor:** DeepBasin
- **Emergence:** MEDIUM — mais HIGH pour la fiabilité/trust

## Priority

Ranked #5 by emergence. Lower creative emergence but critical for system reliability and trust.

## Dependencies

- soullink-perception (port 9033) — fact verification input
- soullink-reasoning (port 9031) — logical consistency checking
- soullink-language (port 9036) — output coherence verification
- sha2 crate — audit trail integrity hashing