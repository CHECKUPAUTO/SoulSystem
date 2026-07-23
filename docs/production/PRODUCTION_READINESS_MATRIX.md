# SoulSystem Production Readiness Matrix

Baseline: `9d2f82783d87c3dad50eade02ce2c96d90c628f5`.

Readiness verdict for the whole system: **NOT_READY**.

This matrix tracks each production-readiness dimension. A dimension is `READY`
only when it has objective, tested evidence in-tree. PR A establishes the
containment guard and the tracking framework; it does not by itself make any
dimension `READY`.

Legend: `READY` · `PARTIAL` · `NOT_READY`.

| # | Dimension | Requirement | State | Evidence / gap | PR |
|---|-----------|-------------|-------|----------------|----|
| 1 | Explicit run mode | Production is explicit, never inferred | PARTIAL | Guard resolves mode explicitly; unset ⇒ development + warning | A |
| 2 | Fail-closed startup | Missing prerequisites abort production start | PARTIAL | Guard implemented; deep enforcement pending in B–K | A |
| 3 | Untrusted input isolation | No untrusted input reaches process exec | NOT_READY | Dispatch/registry not yet hardened | B, C, D |
| 4 | Tool registration | Every tool typed and registered | NOT_READY | — | B |
| 5 | Capability gating | Every side effect capability-checked | NOT_READY | — | C |
| 6 | OS isolation | Mandatory sandbox for all execution | NOT_READY | — | D |
| 7 | Filesystem confinement | Writes confined to canonical roots | NOT_READY | — | E |
| 8 | Authentication | All state-changing endpoints authenticated | NOT_READY | — | F |
| 9 | Transport security | Non-loopback requires TLS | PARTIAL | Guard rejects non-loopback-without-TLS in prod; active TLS path pending | A, G |
| 10 | Webhook verification | Signatures verified, fail closed | NOT_READY | — | F |
| 11 | Memory trust | Screen before persist; provenance | NOT_READY | — | H |
| 12 | Persistence durability | Atomic, integrity-checked, recoverable | NOT_READY | — | L |
| 13 | Self-modification safety | Off by default; PR-only promotion | PARTIAL | Guard rejects unsigned self-mod in prod; safe flow pending | A, K |
| 14 | Secret handling | Zeroizing/redacting types; no leaks | PARTIAL | Guard is secret-free; crate-wide sweep pending | A, J |
| 15 | Planner/metrics truth | Real outcomes reported | NOT_READY | — | I |
| 16 | Single canonical runtime | One supported runtime & deploy path | NOT_READY | — | N |
| 17 | Truthful capabilities | No simulated/placeholder success in prod | NOT_READY | — | O |
| 18 | Observability | Structured audit/metrics; health/readiness | NOT_READY | Guard emits structured startup audit events | A, M |
| 19 | CI production gates | deny/audit/MSRV/fuzz/release/container | NOT_READY | — | P |
| 20 | Operability | Install/config/start/monitor/upgrade/backup/restore/stop | NOT_READY | — | P |

## How to read this

- **PARTIAL** rows are where PR A has begun containment but the durable fix lands
  in a later PR. They must not be reported as `READY`.
- The overall verdict remains **NOT_READY** until every dimension has tested
  evidence, per the security completion gate in the hardening plan.

## Backup / restore / upgrade evidence

None yet. Populated by PR L (persistence) and PR P (deployment) with reproducible
commands and results, and finalized in `PRODUCTION_QUALIFICATION_REPORT.md`.
