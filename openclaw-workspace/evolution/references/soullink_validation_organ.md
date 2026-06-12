# soullink-validation — Vérification et Audit Interne Organ

**Port:** 9044 | **Type:** Vérification et audit interne | **Neurons:** 200 | **Default Attractor:** DeepBasin

**Source:** OpenEvolve Night Cycle 131 (2026-04-14 14:02)

## Raison d'être

Avant toute sortie externe, valider cohérence, fact-check, sécurité. "Conscience critique" du système. Fiabilité critique, mais peu créatif.

## Architecture

```
├── src/main.rs          — Axum HTTP server, port 9044
├── src/verifier.rs      — Output coherence verification
├── src/fact_check.rs    — Cross-reference validation (link to Perception/Reasoning)
├── src/safety_gate.rs   — Safety boundary enforcement
├── src/audit_log.rs     — RocksDB immutable audit trail
├── src/critic.rs        — Internal critic scoring
└── Cargo.toml          — axum, tokio, rocksdb, serde, sha2
```

## API Interfaces

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/validation/verify` | POST | Verify output coherence |
| `/api/validation/factcheck` | POST | Fact-check claim |
| `/api/validation/safety` | POST | Safety boundary check |
| `/api/validation/audit` | GET | Audit trail query |

## Emergence Score: MEDIUM

Fiabilité critique, mais faible potentiel créatif. DeepBasin attractor ensures stable, reliable verification.

## Dependencies

- RocksDB (immutable audit trail)
- sha2 crate (content hashing for audit integrity)
- Links to: Perception organ (9031/9033), Reasoning organ (9032/9036), Language organ (9033/9036)

## Build Estimate

4-6 hours (Rust, Axum, RocksDB, sha2)

## Security Relevance

This organ is critical for system integrity. The safety_gate module should interface with the existing security hardening proposals:
- mTLS between organs (needs manual review)
- Audit trail immuable (this organ provides it)
- Secret rotation via RocksDB encrypted column family