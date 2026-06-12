# OpenClaw Rust Migration Roadmap

**Summary:** Phased Rust migration roadmap for OpenClaw core modules. Phase 1: pure-function modules (config, cron, security, secrets, shared, process, sessions). Phase 2: I/O-bound modules (gateway-core, web-fetch, context-engine, session-store, mcp-protocol).

**Full reference:** [evolution/references/openclaw_rust_migration_roadmap.md](../../evolution/references/openclaw_rust_migration_roadmap.md)

**Key findings:**
- Current: 3.3% OpenClaw core migrated (IronReview stub only)
- P0 first target: `config` module (306 TS files, pure parsing/validation)
- Gateway WS 1006 root cause: port mismatch (now resolved — both at 18890)
- soullink-server-core shared library: -60% boilerplate for new organs
- Unified Cargo workspace consolidation planned
- 6-cycle target: 20 Rust crates, 15,000+ LOC, 100% brain stack

**Created:** 2026-04-14