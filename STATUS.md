# STATUS — SoulSystem Ecosystem Health

*Generated 2026-06-15* (unified workspace merge)

## Merge Summary

The workspace has been reunified into a single buildable Rust tree:

- Restored the full crate list (SoulLink brain, SciRust core, autonomous entity, CCOS, semantic crates, neural crates, bridges).
- Imported `ccos/` as a workspace member.
- Added `scirust-core/` (previously untracked) to version control.
- Removed temporary Python scripts (`select_mtp.py`, `test_soul_mtp.py`) and `Cargo.toml.bak`.
- Upgraded `moka` to `0.12.15` to fix `rustix 0.37` incompatibility with Rust 1.98 nightly.
- Disabled the historical cron scheduler block in `src/main.rs` (`soul_scheduler` is now a work-stealing scheduler; cron behaviour preserved in `soul-daemon`).
- Migrated the `agent-registry/e2e` binary behind an `e2e` feature flag (it references legacy bridge names).
- Fixed `soulsystem-mesh` and `soulsystem-fuzz` dependencies.
- Added a legacy compatibility shim in `soul_llm/src/legacy.rs` so the historical `OllamaClient`/`ChatSession` API keeps the two monoliths compiling without code loss.

## Active Modules

| Module | Status | Dependencies | Notes |
|--------|--------|-------------|-------|
| `soul_memory` | Active | sled | Local vector storage. No Qdrant needed. |
| `telemetry` | Active | tracing-subscriber | OTLP init configurable via `OTEL_EXPORTER_OTLP_ENDPOINT`. |
| `code_signing` | Active | sha2, uuid | ed25519 signature verification. Keys in `~/.soulsystem/authorized_keys`. |
| `audit_log` | Active | sled, sha2, chrono | Immutable hash chain. Storage at `/var/log/soulsystem/audit.sled`. |
| `bus` | Active | tokio broadcast | Internal message bus (256 message buffer). |
| `compute_backend` | Active | — | ComputeBackend trait + CpuFallback. CUDA with `gpu` feature. |
| `config` | Active | toml | `soulsystem.toml` + override via `SOULSYSTEM_*` env vars. |
| `soul_agent_core` | Active | soul_llm, soul_planner, soul_tools | ReAct loop autonomous agent. |
| `soul_llm` | Active | reqwest, serde | Multi-provider LLM client + legacy shim. |
| `soul_planner` | Active | — | Goal decomposition and WorkingMemory. |
| `soul_tools` | Active | soul_sandbox | Permission-gated async tool dispatch. |
| `ccos` | Active | — | Causal Context Operating System (merged). |
| `scirust-core` | Active | — | SciRust scientific computing core. |

## Build

- `cargo check --workspace` : 0 errors
- `cargo test --lib -p scirust-core -p semantic_firewall -p semantic_neuromodulator -p soul_agent_core -p soul_tools -p soul_planner` : ✅ 97/97
- `cargo check -p soullink-inference` : ✅

## Known Caveats

- `soullink-inference` unit tests include long-running `turboquant` tests and are not run by default in CI.
- `agent-registry/e2e` requires the `e2e` feature flag and expects external services to be running.
- Several crates emit `unused_imports`/`dead_code` warnings; these are non-fatal and will be cleaned up incrementally.
- The `souls` dependency in the root `Cargo.toml` currently has no lib target and is ignored by Cargo; it is retained for future integration.
