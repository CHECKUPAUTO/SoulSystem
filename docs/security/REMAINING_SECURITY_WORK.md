# Remaining Security Work

Derived from the re-verification of [`findings.json`](findings.json) against
`main` at `898f4721614a5a2b707641f717aced3fe9679261` (2026-07-28), toolchain
`rustc 1.94.1` / `cargo 1.94.1`.

This document lists only work that current source inspection confirms is still
outstanding. Each item names the finding and invariant IDs it closes, the exact
surface affected, and the acceptance test that would let the corresponding
finding move to `FIXED_AND_VERIFIED`.

## Readiness verdict

**`NOT_READY` for untrusted-network production.**

The verdict follows from the register, not from campaign progress: 2 of 6
critical findings and 3 of 10 high findings remain `PARTIALLY_FIXED`, one high
finding is `CONFIRMED_CURRENT`, and the two P0 items below are both reachable
from the `soulsystem` binary without any feature gate.

`LIMITED_PRODUCTION` is defensible for a **trusted, loopback-only, single-tenant**
deployment where: the process bus cannot be reached by untrusted input, the
gateway is bound to loopback or fronted by an authenticating reverse proxy that
also supplies CORS and request limits, and `--entity` is not used. Those are
operational compensating controls, not proven invariants.

`PRODUCTION_READY` requires at minimum P0 and P1 closed and INV-PERSIST-2
(backup/restore) qualified by an executable test.

## Priority 0 — Production blockers

### P0-1 Unsandboxed, bus-triggered host command execution

- **Findings / invariants:** CRIT-001 (`PARTIALLY_FIXED`), HIGH-002, INV-EXEC-1
- **Surface:** `src/self_healer.rs` (`SelfHealer::execute`), instantiated
  unconditionally at `src/main.rs` daemon startup ("Phase 5") and driven by a
  spawned task subscribed to `error.*` bus messages. Secondary surfaces:
  `soul-automodify` (invokes `cargo`), `soul_gateway` iMessage provider
  (invokes `osascript`).
- **Current risk:** `DefenseAction` handling runs `kill`, `systemctl
  restart/start` and `df` through bare `std::process::Command` — no sandbox, no
  seccomp filter, no output bound, no capability check. Anything that can
  publish an `error.*` bus message, or influence the PIDs and service names
  that reach a `DefenseAction`, reaches host command execution.
- **Recommended PR scope:** route `SelfHealer` command execution through
  `soul_sandbox`, or gate the healer off unless explicitly enabled; then add a
  workspace architecture guard that fails CI on `Command::new` outside the
  approved executor, with an explicit allowlist for build tooling and tests.
- **Dependencies:** none. The sandbox already exists and is proven by
  `soul_sandbox`'s suite.
- **Acceptance tests:** a test asserting a `DefenseAction` cannot spawn an
  unsandboxed process; an architecture test enumerating process-execution call
  sites and failing on any not in the allowlist. The inventory this
  re-verification used (227 matches in the live tree, 110 inside 25
  workspace-member crates) is the starting baseline for that allowlist.
- **Production impact:** removes the highest-severity reachable execution path.

### P0-2 `ws_bridge` fails open when no shared secret is configured

- **Findings / invariants:** CRIT-007 (`PARTIALLY_FIXED`), INV-NET-1
- **Surface:** `src/ws_bridge.rs` — a live module (`pub mod ws_bridge` in
  `src/lib.rs`, used from `src/main.rs`). Its handshake initialises
  `authenticated` to `true` when `shared_secret` is `None` or empty. Related:
  `src/api.rs` builds a `Router` with no authentication layer.
- **Current risk:** with no shared secret set — the default — the bridge accepts
  unauthenticated WebSocket sessions subscribed to the internal bus. Both
  listeners default to loopback (`127.0.0.1:9022`, `127.0.0.1:9023`), so
  exposure requires local access, a rebind or a proxy, but neither fails closed
  and neither is covered by the production startup guard.
- **Recommended PR scope:** invert the default so an unset or empty shared
  secret refuses connections; extend the production guard to treat an unset
  `ws_bridge` secret as a startup violation; add authentication to `src/api.rs`
  or move its routes behind the gateway.
- **Dependencies:** none.
- **Acceptance tests:** a test asserting a connection with no token is rejected
  when `shared_secret` is `None`; a `soul-prod-guard` test asserting production
  startup aborts when the bridge secret is unset.
- **Production impact:** makes the second-largest authenticated surface
  fail-closed, matching the gateway's posture.

## Priority 1 — Major hardening

### P1-1 Webhook signature verification and replay protection

- **Findings / invariants:** HIGH-007 (`PARTIALLY_FIXED`), INV-NET-3
- **Surface:** `soul_gateway/src/lib.rs` webhook handlers; the `decode_hex`
  helpers in `channels/discord.rs` and `channels/whatsapp.rs` already parse
  signature headers but nothing consumes them.
- **Current risk:** handlers fail closed when a secret is unset, but with a
  secret configured a reachable caller can still submit a forged or replayed
  payload — no HMAC comparison, no timestamp window, no nonce cache.
- **Acceptance tests:** per provider, a valid-signature accept, an
  invalid-signature reject, a stale-timestamp reject, and a replayed-nonce
  reject.

### P1-2 CORS allowlist and request/message/concurrency limits

- **Findings / invariants:** INV-NET-4, INV-NET-5 (both `TARGET`)
- **Surface:** `soul_gateway::router`, which applies
  `tower_http::cors::CorsLayer::permissive()` to the merged router — so it
  covers the authenticated `/v1/*` routes, not only `/health`. No
  `DefaultBodyLimit`, concurrency layer, rate limiter, or WebSocket
  max-message bound is present on any gateway path.
- **Acceptance tests:** a disallowed `Origin` is rejected; an oversized body is
  rejected with 413; an oversized WebSocket message is rejected; concurrent
  requests beyond the cap are shed rather than queued unboundedly.

### P1-3 Sandbox resource limits (cgroups) and filesystem/PID namespaces

- **Findings / invariants:** HIGH-003 (`PARTIALLY_FIXED`), INV-EXEC-3,
  INV-EXEC-4
- **Surface:** `soul_sandbox/src/lib.rs`, `policy.rs`.
- **Current risk:** seccomp is mandatory and fail-closed, output is bounded and
  the process group is killed on timeout, but CPU, memory, pids and file
  descriptors are unbounded, and mount/PID namespaces are not applied. Network
  isolation is best-effort by design and silently degrades on hosts where
  unprivileged `CLONE_NEWUSER` is restricted, so egress cannot be relied on as
  a control.
- **Acceptance tests:** a fork bomb is contained by the pids limit; a memory hog
  is OOM-killed inside its cgroup rather than on the host; per-tool egress
  allowlisting is honoured where namespaces are available.

### P1-4 Provider retry, backoff and a coherent cross-layer retry policy

- **Findings / invariants:** MED-001 (`CONFIRMED_CURRENT`), MED-009
  (`PARTIALLY_FIXED`)
- **Surface:** `soul_llm/src/client.rs` and the three providers. A
  `LlmError::RateLimited { retry_after }` variant exists but nothing acts on
  it; the only `tokio::time::sleep` in `client.rs` is inside a `#[cfg(test)]`
  mock provider.
- **Current risk:** one transient upstream failure or 429 aborts an autonomous
  run. Retry behaviour differs by layer with no shared budget, and no layer
  honours `Retry-After`.
- **Acceptance tests:** a failure-injecting mock provider proves bounded
  exponential backoff with jitter, a respected `Retry-After`, and a bounded
  total attempt count shared with the agent loop.

### P1-5 Secret-type sweep beyond `soullink-secrets`

- **Findings / invariants:** HIGH-001 (`FIXED_AND_VERIFIED` for its own crate),
  INV-SEC-1, INV-SEC-2 (both `PARTIAL`)
- **Surface:** gateway and LLM provider configuration structs still hold tokens
  and secrets in ordinary `String` fields.
- **Acceptance tests:** a test asserting `Debug` output for each
  secret-bearing config struct contains no secret material.

### P1-6 Memory provenance and trust metadata

- **Findings / invariants:** INV-MEM-2 (`PARTIAL`), INV-MEM-3 (`TARGET`)
- **Surface:** CCOS causal memory, planner history, vector stores.
- **Current risk:** screening before persistence is enforced by type at the
  `soul-agent-core` call sites (CRIT-005), but persisted records carry no
  provenance or trust level, and crates writing to the same stores directly are
  not type-constrained.
- **Acceptance tests:** every persisted record carries provenance and a trust
  level; a direct-write path cannot bypass screening.

### P1-7 Transactional multi-file persistence and backup/restore qualification

- **Findings / invariants:** HIGH-005 (`FIXED_AND_VERIFIED` for single-file
  atomicity), INV-PERSIST-1 (`PARTIAL`), INV-PERSIST-2 (`TARGET`)
- **Current risk:** each CCOS file is written atomically, but a crash between
  two related writes can leave the three files mutually inconsistent. There is
  no corruption detection on load and no state versioning. No executable test
  proves a backup can be taken, state destroyed, and the backup restored.
- **Acceptance tests:** an integration test performing backup → destroy →
  restore → integrity check; a torn-multi-file-write test proving detection.

## Priority 2 — Product decisions

These need an owner, not an engineer. Each is `DEFERRED_PRODUCT_DECISION` or
blocked behind one in the register.

### P2-1 Canonical agent runtime

- **Findings / invariants:** HIGH-008 (`DEFERRED_PRODUCT_DECISION`), INV-EXEC-5
- **Decision required:** which runtime is canonical; whether `--entity` and the
  standalone `soul-kernel` binary remain supported; whether the duplicate
  Telegram polling path (`clawd::run_bot` and the `soul_gateway` Telegram
  provider can poll the same token) is removed.
- **Why it blocks:** every additional runtime carries an independent security
  posture. Controls proven for `AutonomousAgent` — sandboxed dispatch, budgets,
  emergency stop — are not automatically true for `SoulEntity` or
  `soul-kernel`, which contributes 22 of the process-execution call sites.

### P2-2 Disposition of simulated and placeholder features

- **Findings / invariants:** HIGH-006 (`CONFIRMED_CURRENT`), MED-004
  (`CONFIRMED_CURRENT`), LOW-006, MED-010, LOW-004, LOW-005, INV-TRUTH-1/2
- **Decision required:** for each of `--entity` simulated plan execution,
  Tree-of-Thoughts placeholder embeddings, `--plan` keyword planning, the WASM
  host stubs and the CPU-delegating GPU backends — implement, remove, or
  experimental-gate. Both HIGH-006 and MED-004 are reachable today and report
  success or semantic scores they do not earn.

### P2-3 Global lint policy

- **Findings / invariants:** LOW-001 (`DEFERRED_PRODUCT_DECISION`)
- **Decision required:** whether to reverse the workspace-wide suppression of
  `dead_code` / `unused_imports` / `unused_variables` / `deprecated`. Under CI's
  `-D warnings` this would convert an unknown, potentially very large number of
  currently-masked warnings across 184 packages into hard failures, so it needs
  a triage budget and a staged plan.
- **Security relevance:** the suppression is what allows orphaned security
  components (HIGH-002) to accumulate without a build-time signal.

### P2-4 SBOM, artifact signing and provenance trust model

- **Findings / invariants:** INV-CI-3 (`PARTIAL`)
- **Decision required:** whether to provision a signing key as a repository
  secret and who owns it. `release.yml` already emits SHA-256 checksums.
  GitHub's `actions/attest-build-provenance` would add provenance with no new
  secrets and is the lowest-risk first step.

## Priority 3 — Hygiene

| Item | Findings | Action |
|---|---|---|
| Empty stub integration-test crate | LOW-007 | Delete `integration_tests/Cargo.toml` (the only tracked file in that directory) or populate it with real tests and add it to the workspace. It currently advertises multi-crate integration coverage that does not exist. |
| Root package version drift | LOW-002 | Root `soulsystem` is `0.6.0` while `[workspace.package]` is `13.5.0`; the banner, `/health`, metrics and backup metadata all report `0.6.0`. Set `version.workspace = true` and add a manifest-consistency CI check. |
| Dead orphaned module | LOW-006 | `src/autonomous_loop.rs` is not declared in `src/lib.rs` or `src/main.rs`, so it is not compiled — including its `0.0.0.0` listener bind. Delete it so the unreachable bind cannot be revived by adding a `mod` line. |
| Unreachable unauthenticated MCP server | MED-007 | `soul_mcp::serve_ws` dispatches `tools/call` with no peer authentication and has no production caller. Feature-gate it as experimental before any caller is introduced. |
| Excluded-tree duplicates | LOW-003 | The `os-agents/` copy of `check.sh` still hardcodes the original absolute path. Out of workspace and not built by CI; resolve if that tree is ever revived. |

## How to re-run this verification

```bash
python3 scripts/validate-security-findings.py   # register consistency
cargo deny --workspace check                    # supply chain
rustup run 1.93.0 cargo check --workspace --all-targets   # declared MSRV
```

Every `FIXED_AND_VERIFIED` finding in `findings.json` carries a
`verification_tests` array whose `command` fields are individually runnable;
all 36 were executed and passed at `898f472`.
