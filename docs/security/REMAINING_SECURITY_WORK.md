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
critical findings and 3 of 10 high findings remain `PARTIALLY_FIXED`, and one
high finding is `CONFIRMED_CURRENT`. **Both P0 items are closed, and P1-1 and
P1-2 are closed**, but the verdict does not move to `LIMITED_PRODUCTION` on that
alone: the P1 set below still contains unauthenticated surface (`src/api.rs`),
unbounded sandbox resources, unbounded connection counts and no per-client rate
limiting.

`LIMITED_PRODUCTION` is defensible for a **trusted, loopback-only, single-tenant**
deployment where: the process bus cannot be reached by untrusted input, the
gateway is bound to loopback or fronted by an authenticating reverse proxy that
supplies connection limits and rate limiting, and `--entity` is not used. Those
are operational compensating controls, not proven invariants. Note that CORS and
request-body limits no longer belong on that list — the gateway now enforces
both itself.

`PRODUCTION_READY` requires at minimum P0 and P1 closed and INV-PERSIST-2
(backup/restore) qualified by an executable test.

## Priority 0 — Production blockers *(all closed)*

### ~~P0-1 Unsandboxed host command execution in the binary~~ — CLOSED

- **Findings / invariants:** CRIT-001, HIGH-002, INV-EXEC-1
- **Status:** closed by the `security/p0-1-sandbox-self-healer` change. Note this
  also **corrected an overstatement** in the first re-verification: of the three
  spawn sites in `src/self_healer.rs`, only `df` was ever live (via the
  30-second `run()` loop). The `kill` and `systemctl` arms had **no producer** —
  `Preservation` never constructs `KillNonEssential` or `RestartService` — so
  they were latent, not attacker-reachable as originally written.
- **What changed:** the `df` spawn is replaced by `statvfs(3)`
  (`SelfHealer::root_disk_used_percent`), removing the exec rather than
  sandboxing it; `kill(1)` is replaced by a direct `libc::kill` signal; both
  privileged arms are gated behind `ProcessControl::Enabled` (default
  `Disabled`), so adding a producer cannot by itself turn bus traffic into host
  process control; the systemd unit name is validated by `is_safe_service_name`.
- **Guard:** `tests/architecture_process_execution.rs` pins the permitted set of
  process-spawning files in the binary crate, verifies each allowlist entry is
  still justified, and asserts disk telemetry never shells out again. Its
  failure path was verified by introducing a deliberate violation.
- **Remaining (moved to P1-8):** the guard covers `src/` only.

### ~~P0-2 `ws_bridge` fails open when no shared secret is configured~~ — CLOSED

- **Findings / invariants:** CRIT-007, INV-NET-1
- **Status:** closed by the `security/p0-2-ws-bridge-fail-closed` change.
- **What changed:** `WsBridgeConfig` gained `UnauthenticatedAccess` (default
  `Deny`), so with no usable secret `handle_connection` refuses before the
  WebSocket handshake instead of initialising `authenticated = true`. A blank or
  whitespace-only secret is treated as unset by `effective_secret()`, so an
  empty config value cannot become a credential. The token comparison is now
  constant-time — the previous `token == secret` was a timing side channel the
  gateway had already closed for its own bearer token. Serving unauthenticated
  requires an explicit `SOULSYSTEM_WS_BRIDGE_ALLOW_UNAUTHENTICATED` opt-in, and
  that opt-in is deliberately **not** reported as authentication.
- **Guard wiring:** `src/prod_guard.rs` no longer hardcodes the bridge posture —
  `assemble_posture` receives the real `WsBridgeConfig`, so production startup
  aborts with an `UnauthenticatedListener` violation while the bridge is
  unauthenticated.
- **Remaining (moved to P1-9):** `src/api.rs` still has no auth layer, and
  per-scope authorization is unimplemented.

## Priority 1 — Major hardening

### ~~P1-1 Webhook signature verification and replay protection~~ — CLOSED

- **Findings / invariants:** HIGH-007 (now `FIXED_AND_VERIFIED`), INV-NET-3
- **Status:** closed by `security/p1-1-webhook-signature-verification`.
- **Correction to this roadmap's own text:** it said "per-provider HMAC-SHA256".
  That is wrong for Discord, which uses **Ed25519** over `timestamp ‖ body`,
  with `DISCORD_PUBLIC_KEY` being a hex *public key* rather than a shared
  secret. Slack (HMAC-SHA256 over `v0:{ts}:{body}`) and Meta/WhatsApp
  (HMAC-SHA256 over the raw body) do use HMAC.
- **What changed:** the handlers used axum's `Json(payload)` extractor, which
  consumes the body — signature verification needs the raw bytes, so they now
  take `HeaderMap` + `Bytes`, verify, and only then deserialize. HMAC
  comparisons use `subtle::ConstantTimeEq`; Ed25519 is constant-time by
  construction. Timestamp freshness is enforced within `MAX_SKEW` (5 min, both
  directions) where the provider sends one. A `ReplayCache` on `GatewayState`
  rejects any already-accepted signature and evicts entries past `MAX_SKEW` so
  it stays bounded. Every rejection returns one opaque 401, logged server-side
  only, so a caller cannot distinguish a bad signature from a stale timestamp.
- **Residual:** the replay cache is per-process and in-memory, so a
  multi-instance deployment would not share it — a captured request could be
  replayed once per instance inside the freshness window. Verification is
  unit-tested against real algorithm output (including genuine Ed25519
  signing), not against live provider traffic.

### ~~P1-2 CORS allowlist and request/message/concurrency limits~~ — CLOSED

- **Findings / invariants:** INV-NET-4, INV-NET-5 (both now `PARTIAL`, not
  `HELD` — see the residuals below and the two follow-up items P1-10 / P1-11)
- **Status:** closed by `security/p1-2-cors-and-request-limits`.
- **What changed — CORS:** `soul_gateway::limits::CorsPolicy` replaces
  `CorsLayer::permissive()`. The allowlist comes from
  `SOULSYSTEM_GATEWAY_CORS_ORIGINS`; unset, blank, and a bare `*` all resolve to
  `CorsPolicy::Disabled`, which emits no `Access-Control-Allow-Origin` header at
  all. `*` is filtered rather than honoured deliberately: re-creating the
  permissive behaviour should require enumerating origins, not typing one
  character. There is therefore no reachable permissive state. The layer is
  applied outermost so 401 and 413 responses carry the same origin decision as
  successful ones.
- **What changed — limits:** `DefaultBodyLimit::max` (1 MiB default) on every
  route, `GlobalConcurrencyLimitLayer` (64 default) across all routes, and
  `max_message_size`/`max_frame_size` (256 KiB default) on the `/v1/stream`
  upgrade, read off `GatewayState::limits`. All three come from the environment
  and a malformed or zero value falls back to the default — `MAX_BODY=abc` must
  not silently become "reject everything", which would present as an outage
  rather than a misconfiguration.
- **Two implementation notes worth recording:**
  - `GlobalConcurrencyLimitLayer`, not `ConcurrencyLimitLayer`. The latter
    constructs a fresh semaphore on every `Layer::layer` call, and
    `Router::layer` calls it once per route — so the real ceiling would have
    been N × routes rather than N. This was verified, not assumed: swapping in
    `ConcurrencyLimitLayer` makes `the_concurrency_budget_is_shared_across_routes`
    fail.
  - The body bound also protects the *unauthenticated* webhook surface, because
    axum runs extractors before the handler body — so an oversized
    `/providers/slack/webhook` request returns 413 and never reaches the
    signature check.
- **Deviation from this roadmap's own acceptance criteria:** it asked that
  "concurrent requests beyond the cap are shed rather than queued unboundedly".
  The implementation **queues** rather than sheds: excess requests wait on the
  shared semaphore and are served when a permit frees. Shedding with 429 was
  rejected because the gateway's clients are operators and channel providers,
  for whom a dropped request is a lost instruction, whereas added latency is
  recoverable. The queue is not itself bounded, which is exactly residual (a)
  below.
- **Also fixed, outside the original scope:** `soul-dashboard` applied
  `CorsLayer::permissive()` too, and it is reachable from the production binary
  (`src/main.rs:1230`, whenever `--dashboard-port > 0` or `--dev`). Binding to
  `127.0.0.1` does not make that safe — a page in the operator's browser can
  read `http://127.0.0.1:<port>/api/*` cross-origin precisely because of the
  `*` header, and `src/main.rs:1235` calls `run_server(..., None)`, i.e. with no
  auth token. It now uses the same allowlist shape via `cors_layer_from_env`
  and `SOULSYSTEM_DASHBOARD_CORS_ORIGINS`. The policy is a ~30-line local mirror
  rather than a shared type: giving `soul-dashboard` a dependency on
  `soul_gateway` would invert the layering, and hoisting the type into
  `soulsystem-common` would pull `tower-http` into a crate that does not
  otherwise need HTTP.
- **Residual (a) — no connection bound.** The concurrency layer limits work in
  flight, not accepted connections or queued requests. A slow-loris style
  connection flood is not addressed. Tracked as P1-11.
- **Residual (b) — no per-client rate limiting** on `soul_gateway`.
  `soul-dashboard` has a `SimpleRateLimiter`; the gateway has none. Tracked as
  P1-11.
- **Residual (c) — the other listeners are untouched.** `src/api.rs` and
  `src/ws_bridge.rs` carry no body, message or concurrency limits at all.
  Tracked as P1-11.
- **Residual (d) — four other crates still call `CorsLayer::permissive()`:**
  `soullink-gateway` (`src/cli/run.rs`), `soullink-inference`
  (`src/bin/turboquant-proxy.rs`), `soullink-orchestrator-v3` (`src/main.rs`)
  and `soulsystem-lite` (`src/main.rs`). Each was checked against
  `cargo tree -p soulsystem --edges normal`; none is in the production binary's
  dependency graph, so none is reachable from `soulsystem`. They are still real
  services if deployed on their own. Tracked as P1-10.

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

### P1-9 Authenticate `src/api.rs` and add per-scope authorization

- **Findings / invariants:** CRIT-007 (residual), INV-NET-1
- **Surface:** `src/api.rs` builds a `Router` with no authentication layer,
  served on `127.0.0.1:9023`. Separately, the gateway's bearer token is
  all-or-nothing: every authenticated caller has full operator power.
- **Current risk:** the API listener is reported to the guard as
  unauthenticated (so production startup fails on it), but it has no auth of its
  own and is mitigated only by the loopback bind.
- **Acceptance tests:** an unauthenticated request to a state-changing `api`
  route is rejected; a token scoped to read-only cannot reach a write route.

### P1-8 Widen the process-execution guard beyond the binary crate

- **Findings / invariants:** CRIT-001 (residual), HIGH-002, INV-EXEC-1
- **Surface:** 110 process-execution matches across 25 workspace-member crates,
  including 22 in the separate `soul-kernel` binary. Also `soul-automodify`
  (invokes `cargo`) and the `soul_gateway` iMessage provider (invokes
  `osascript`), both outside the sandbox.
- **Why it is P1 and not P0:** the binary's only *live* unsandboxed spawn is
  gone and new ones in `src/` now fail CI. The rest is unenforced surface, not a
  demonstrated reachable path.
- **Recommended PR scope:** classify each match (approved sandbox impl /
  test-only / build tooling / supported production path / experimental /
  unreachable), then extend the existing guard's allowlist model workspace-wide.
- **Acceptance tests:** the guard runs over all workspace members with a
  justified allowlist, and fails on an introduced violation in any crate.

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

### P1-10 Permissive CORS in the four non-production HTTP services

- **Findings / invariants:** INV-NET-4 (`PARTIAL`)
- **Surface:** `soullink-gateway` (`src/cli/run.rs`), `soullink-inference`
  (`src/bin/turboquant-proxy.rs`), `soullink-orchestrator-v3` (`src/main.rs`)
  and `soulsystem-lite` (`src/main.rs`) all call
  `tower_http::cors::CorsLayer::permissive()`.
- **Why it is P1 and not P0:** each was checked against
  `cargo tree -p soulsystem --edges normal` and none appears in the production
  binary's dependency graph, so none is reachable from `soulsystem`. They are
  independently deployable services, so the defect is real but not on the
  production path.
- **Recommended PR scope:** the same allowlist shape as P1-2. If a third copy
  is needed, hoist the policy into a small shared crate rather than mirroring it
  again — two copies is the point at which duplication is still cheaper than
  the dependency edge, three is not.
- **Acceptance tests:** per service, a disallowed `Origin` receives no
  `Access-Control-Allow-Origin`, and a bare `*` in config does not re-enable
  permissive behaviour.

### P1-11 Connection bounds, rate limiting, and limits on the other listeners

- **Findings / invariants:** INV-NET-5 (`PARTIAL`)
- **Surface:** `soul_gateway` (connection count, per-client rate), `src/api.rs`
  and `src/ws_bridge.rs` (no limits of any kind).
- **Current risk:** P1-2 bounds request *bodies*, *messages* and *work in
  flight* on the gateway. It does not bound accepted connections, so a
  slow-loris style flood still ties up sockets and grows the wait queue behind
  the concurrency semaphore; and there is no per-client rate limit, so one
  authenticated operator token can saturate the whole budget. `src/api.rs` and
  `src/ws_bridge.rs` were not touched at all.
- **Acceptance tests:** connections beyond the cap are refused rather than
  accepted-and-parked; a single client exceeding its rate is throttled without
  affecting others; `src/api.rs` and `src/ws_bridge.rs` reject an oversized
  body and an oversized WebSocket frame.

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
