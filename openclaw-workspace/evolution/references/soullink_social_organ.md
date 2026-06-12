# soullink-social — Intelligence Sociale et Théorie de l'Esprit Organ

**Port:** 9043 | **Type:** Intelligence sociale et théorie de l'esprit | **Neurons:** 300 | **Default Attractor:** Transient

**Source:** OpenEvolve Night Cycle 131 (2026-04-14 14:02)

## Raison d'être

Modéliser les états mentaux des interlocuteurs. Adapter ton, style, timing. Critique pour interactions multi-utilisateurs. Enrichit massivement les interactions humaines.

## Architecture

```
├── src/main.rs            — Axum HTTP server, port 9043
├── src/theory_of_mind.rs  — Mental state modeling for interlocutors
├── src/style_adapter.rs   — Communication style matching
├── src/context_social.rs  — Social context tracking (group dynamics, hierarchies)
├── src/empathy.rs         — Affective empathy simulation (linked to Affect organ)
├── src/social_db.rs       — RocksDB interlocutor profiles
└── Cargo.toml            — axum, tokio, rocksdb, serde, chrono
```

## API Interfaces

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/social/model` | POST | Model interlocutor mental state |
| `/api/social/adapt` | POST | Adapt response style to interlocutor |
| `/api/social/context` | GET | Current social context |
| `/api/social/profile` | POST | Update interlocutor profile |

## Emergence Score: HIGH

Enrichit massivement les interactions humaines. Transient attractor allows rapid context switching between interlocutors.

## Dependencies

- RocksDB (interlocutor profiles)
- chrono crate (temporal social patterns)
- Links to: Affect organ (9034/9035), Language organ (9036/9033), Integration organ (9032/9036)

## Build Estimate

5-7 hours (Rust, Axum, RocksDB, social modeling)

## Neural Model Notes

Transient attractor enables rapid mental-model switching between different interlocutors. The theory_of_mind module tracks beliefs, desires, and intentions of conversation partners.