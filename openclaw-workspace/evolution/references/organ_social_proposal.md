# soullink-social — Social Intelligence Organ

**Port:** 9043  
**Type:** Intelligence sociale et théorie de l'esprit  
**Emergence Score:** HIGH  
**Status:** Proposal (not yet implemented)  
**Source:** night_cycle_20260414_1402.md  
**Created:** 2026-04-14

## Raison d'être

Modéliser les états mentaux des interlocuteurs. Adapter ton, style, timing. Critique pour interactions multi-utilisateurs.

## Architecture

```
soullink-social/
├── src/main.rs            — Axum HTTP server, port 9043
├── src/theory_of_mind.rs  — Mental state modeling for interlocutors
├── src/style_adapter.rs  — Communication style matching
├── src/context_social.rs  — Social context tracking (group dynamics, hierarchies)
├── src/empathy.rs         — Affective empathy simulation (linked to Affect organ)
├── src/social_db.rs       — RocksDB interlocutor profiles
└── Cargo.toml            — axum, tokio, rocksdb, serde, chrono
```

## API Interfaces

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/social/model` | Model interlocutor mental state |
| POST | `/api/social/adapt` | Adapt response style to interlocutor |
| GET | `/api/social/context` | Current social context |
| POST | `/api/social/profile` | Update interlocutor profile |

## Neural Configuration

- **Neurons:** 300
- **Default Attractor:** Transient
- **Emergence:** HIGH — enrichit massivement les interactions humaines

## Priority

Ranked #3 by emergence. Essential for multi-user interactions and group chat dynamics.

## Dependencies

- soullink-affect (port 9034) — empathy simulation link
- soullink-language (port 9036) — communication style adaptation
- soullink-memory (port 9030) — interlocutor profile persistence