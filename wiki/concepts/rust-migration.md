# Rust Migration

_Concept page — Python → Rust migration patterns and status._

## Context
SoulLink V12 was Python (LIF vectorized NumPy + TurbulenceEngine SIMD). V13 migrated entirely to Rust. OpenClaw itself has started a partial Rust migration.

## SoulLink V13
- **Status**: Complete (2026-04-12)
- **6 nodes**: soullink-node binary compiled, each on own port (9010-9015)
- **Orchestrator**: Rust v3 (axum + tokio + dashmap) on port 9020
- **Legacy**: All Python archived in `_archive_legacy_python/`
- **Systemd**: sl13-brain-* services, legacy disabled

## OpenClaw Rust Migration
- **Phase 1**: 5 crates compiled (1063 lines)
  - session-store (152l) — P0 CRUD SQLite WAL
  - gateway-core (236l) — P1 HTTP/WS routing
  - plugin-runtime (171l) — P1 Registry + loader
  - agent-pipeline (205l) — P2 Model registry
  - config-parser (178l) — P2 YAML/JSON validation
- **Phase 2**: In progress (bindings napi-rs, tests, docs)

## Common Bugs (Python → Rust)
- Duplicate deps in Cargo.toml
- Trait bounds not satisfied
- Utf8Bytes type mismatches
- Async executor refactoring needs

## Lessons
- session-store has Cargo.toml but 0 lines of actual Rust code written
- Must verify file existence before claiming completion
- No simulating metrics on unwritten code

## See Also
- [soullink](../entities/soullink.md)
- [openevolve](../entities/openevolve.md)