# STATUS — SoulSystem Ecosystem Health

*Generated 2026-05-16*

## Active Modules

| Module | Status | Dependencies | Notes |
|--------|--------|-------------|-------|
| `soul_memory` | ✅ Active | sled | Local vector storage. No Qdrant needed. |
| `telemetry` | ✅ Active | tracing-subscriber | OTLP init configurable via `OTEL_EXPORTER_OTLP_ENDPOINT`. |
| `code_signing` | ✅ Active | sha2, uuid | ed25519 signature verification. Keys in `~/.soulsystem/authorized_keys`. |
| `audit_log` | ✅ Active | sled, sha2, chrono | Immutable hash chain. Storage at `/var/log/soulsystem/audit.sled`. |
| `bus` | ✅ Active | tokio broadcast | Internal message bus (256 message buffer). |
| `compute_backend` | ✅ Active | — | ComputeBackend trait + CpuFallback. CUDA with `gpu` feature. |
| `config` | ✅ Active | toml | `soulsystem.toml` + override via `SOULSYSTEM_*` env vars. |

## Idle Modules (integrated, disableable)

| Module | Status | Reactivation Condition |
|--------|--------|------------------------|
| `federated_learning` | ⏸️ Idle | When a second SoulSystem instance is deployed. |
| `meta_learning` | ⏸️ Idle | When OpenEvolve is integrated as a direct dependency. |
| `dev_dashboard` | ⏸️ Idle | `--dev` flag on launch (feature `dev`). |
| `discovery` | ⏸️ Idle | When mDNS is needed (multi-instance LAN). |
| `soul_wallet` | ⏸️ Idle | When a Lightning node is available. |
| `swarm` | ⏸️ Idle | When 3+ instances are deployed. |
| `jit_hnn` | ⏸️ Idle | Feature `jit` (Cranelift) — heavy dependencies. |
| `hardware_autoscaler` | ⏸️ Idle | Monitoring mode only. |

## Backlog Modules (documented, not integrated)

| Module | Repository | Documentation | Priority |
|--------|-----------|---------------|----------|
| `skill_marketplace` | SoulSystem | `docs/SKILL_MARKETPLACE.md` | Low |
| `skill_api` | SoulSystem | `docs/SKILL_MARKETPLACE.md` | Low |
| Python/TS SDK | SoulSystem | `sdk/README.md` | Low |
| Nix sandbox | SoulSystem | `docs/NIX_SANDBOX.md` | Low |
| Anomaly detection | SYNERGIE | `README.md` | Medium |
| Quantization int8 | scirust | `README.md` | High (Jetson AGX) |
| Homomorphic | scirust | `README.md` | Low |

## Test Results

| Suite | Tests | Result |
|-------|-------|--------|
| audit_log_test | 2 | ✅ |
| bus_test | 2 | ✅ |
| code_signing_test | 2 | ✅ |
| federated_test | 3 | ✅ |
| meta_learning_test | 1 | ✅ |
| soul_memory_test | 3 | ✅ |
| lib (unit) | 11 | ✅ |
| integration_hello | 0 | ⏸️ (placeholders) |

## Build

- `cargo build` : ✅ 0 errors
- `cargo test` : ✅ 26/26
- `cargo build --release` : ✅