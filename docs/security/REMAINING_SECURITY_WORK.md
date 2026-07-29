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
critical findings and 2 of 10 high findings remain `PARTIALLY_FIXED`, and one
high finding is `CONFIRMED_CURRENT`. **Both P0 items are closed, and P1-1
through P1-4 are closed**, but the verdict does not move to
`LIMITED_PRODUCTION` on that alone: the P1 set below still contains
unauthenticated surface (`src/api.rs`), an unbounded sandbox process count, no
filesystem or PID isolation for sandboxed commands, unbounded connection counts
and no per-client rate limiting.

`LIMITED_PRODUCTION` is defensible for a **trusted, loopback-only, single-tenant**
deployment where: the process bus cannot be reached by untrusted input, the
gateway is bound to loopback or fronted by an authenticating reverse proxy that
supplies connection limits and rate limiting, and `--entity` is not used. Those
are operational compensating controls, not proven invariants.

Three things have moved off that compensating-controls list and are now enforced
in-tree: CORS and request-body limits (P1-2), sandboxed CPU/memory/fd/file-size
ceilings (P1-3), and provider retry with backoff (P1-4). Process-count limiting
for sandboxed commands has **not** — `RLIMIT_NPROC` is per-UID, so it is off by
default and a dedicated UID or a cgroup remains an operational prerequisite.

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

### ~~P1-3 Sandbox resource limits~~ — CLOSED for CPU/memory/fd/file-size

- **Findings / invariants:** HIGH-003 (still `PARTIALLY_FIXED`), INV-EXEC-4
  (still `PARTIAL`)
- **Status:** closed by `security/p1-3-sandbox-resource-limits` **for the
  resource-ceiling half only**. The namespace half is not done and is split out
  as P1-12 below, with the reasons. This item's title said "(cgroups) and
  filesystem/PID namespaces"; both of those words turned out to be wrong for
  this codebase and the correction is recorded here rather than quietly.
- **Correction — `setrlimit`, not cgroups.** cgroup v2 can only bound a process
  the caller can place in a cgroup it controls, which needs root or an
  explicitly delegated subtree (`Delegate=yes`, or a rootless systemd user
  slice). SoulSystem is routinely run as an unprivileged user with no
  delegation — the same class of host that already defeats `CLONE_NEWUSER` for
  network isolation. `setrlimit(2)` needs no privilege to *lower* a limit and is
  inherited across `exec`, so it is the bound that actually applies where we
  run. cgroups remain the better mechanism where available; this does not
  implement them.
- **What changed:** `SandboxPolicy::resource_limits` (`ResourceLimits`) is
  applied in `pre_exec` on every spawn path. Defaults: `RLIMIT_AS` 4 GiB,
  `RLIMIT_NOFILE` 256, `RLIMIT_CPU` 60 s, `RLIMIT_FSIZE` 256 MiB, `RLIMIT_CORE`
  0. Both the soft *and* the hard limit are lowered, or the sandboxed process
  could raise its own soft limit straight back to the inherited hard one. The
  desired value is clamped to the inherited hard limit rather than used
  directly, so a host that already runs us under a tighter ceiling keeps that
  ceiling instead of the call failing `EPERM`.
- **Fail-closed, unlike network isolation.** Lowering an rlimit needs no
  privilege and has no host-policy dependency, so a failure is a bug rather than
  an environment to tolerate: `pre_exec` returns `Err`, which aborts the fork
  before `exec`.
- **Residual (a) — pids are still unbounded by default.** `RLIMIT_NPROC` is
  counted per real UID, not per process tree. A non-`None` default would make
  the first `fork` of an ordinary command fail with `EAGAIN` on any shared-UID
  host (workstation, CI runner) while doing nothing to bound a determined
  caller, because the ambient process count already exceeds any useful ceiling.
  It is available as `ResourceLimits::max_processes` and proven to reach the
  child, but it is only safe under a dedicated UID. For a shared UID the correct
  control is a cgroup `pids.max`. Tracked as P1-12.
- **Residual (b) — `RLIMIT_AS` bounds virtual address space, not resident
  memory.** A process that reserves large sparse mappings is refused even if it
  would never fault them in; conversely nothing here reacts to actual RSS. A
  cgroup `memory.max` is the mechanism that measures the thing we care about.
- **Residual (c) — `RLIMIT_CPU` is CPU time, not wall time**, so it does not
  replace `policy.timeout`; a process that sleeps forever burns no CPU. The
  default (60 s) is deliberately looser than the default 30 s wall timeout so an
  ordinary overrun is terminated by the timeout path, which produces a proper
  verdict, rather than by `SIGXCPU`.
- **Acceptance evidence:** five tests, four of which read the child's real
  `/proc/self/limits` rather than asserting on the policy struct. The whole
  suite was additionally re-run under `setpriv --reuid nobody` (41 passed),
  because the unprivileged path is the one that matters and the last sandbox
  change shipped a bug that only appeared there.

### P1-12 Sandbox PID/mount namespaces, cgroup pids/memory, per-tool egress

- **Findings / invariants:** HIGH-003 (`PARTIALLY_FIXED`), INV-EXEC-3
  (`PARTIAL`), INV-EXEC-4 (`PARTIAL`)
- **Surface:** `soul_sandbox/src/lib.rs` — `apply_sandbox_pre_exec` and the
  `Command`-based spawn model itself.
- **Why the PID namespace is not a flag.** `unshare(CLONE_NEWPID)` does **not**
  move the calling process into the new namespace; it only affects children
  created *afterwards*. In `pre_exec` the next thing that happens is `execve`,
  not `fork`, so the exec'd program would stay in the parent's PID namespace and
  the new one would have no members at all. Making it real requires the child to
  fork again after unsharing, with the grandchild as PID 1 and the intermediate
  process reaping it — which `std::process::Command` does not model: the
  returned `Child` would refer to the intermediate, changing exit-status
  reporting and the `killpg` timeout path. That is a restructure of the spawn
  model, not an added syscall. It also needs `CAP_SYS_ADMIN` or a user
  namespace, i.e. the same host dependency that already makes network isolation
  best-effort.
- **Why the mount namespace is not a flag.** `unshare(CLONE_NEWNS)` alone
  changes nothing observable — restricting what the process can see also needs a
  prepared root (`pivot_root`/`chroot`) with the binary and its shared libraries
  bound in. Deciding what a sandboxed program may see is a filesystem-policy
  question, and `SandboxPolicy` currently expresses path policy as string
  matching over the command line, not as a filesystem view. The policy model has
  to exist before the namespace is useful.
- **Recommended PR scope:** (1) a cgroup v2 backend used when a delegated
  subtree is available, supplying `pids.max` and `memory.max` and falling back
  to the rlimits from P1-3 otherwise — with the availability check reported, not
  silent; (2) the double-fork spawn restructure for `CLONE_NEWPID`; (3) a
  filesystem-view policy, then `CLONE_NEWNS` on top of it; (4) per-tool egress
  allowlisting to replace the current all-or-nothing network namespace.
- **Acceptance tests:** a fork bomb is contained without relying on a dedicated
  UID; a memory hog is killed by its own cgroup rather than by the host OOM
  killer; the sandboxed process sees only the paths its policy grants; a tool
  without egress cannot reach the network while one with it can.

### ~~P1-4 Provider retry and backoff~~ — CLOSED for the provider layer

- **Findings / invariants:** MED-001 (now `FIXED_AND_VERIFIED`), MED-009 (still
  `PARTIALLY_FIXED`), INV-PROVIDER-2 (`HELD`), INV-PROVIDER-3 (`PARTIAL`)
- **Status:** closed by `security/p1-4-provider-retry-backoff` **for the
  provider layer**. The cross-layer half is mechanism-only and is split out as
  P1-13 below.
- **Correction to the register's own text.** MED-001 said "a `RateLimited`
  error variant exists but nothing acts on it". Nothing ever *constructed* it
  either. None of the three providers inspected the HTTP status — every call
  was `.send().await?.json().await?`, so a 429 or a 503 reached `serde_json` as
  an HTML error page and surfaced as `LlmError::Serialization`. Retrying on
  that classification would have retried on the wrong signal, so the status
  check had to land in the same change.
- **What changed — classification:** all 16 provider request sites go through
  `http::SendChecked::send_checked`. An extension trait rather than a free
  function, so the call sites stay ordinary builder chains and a newly added
  request cannot skip the check by being written the old way. Status maps in
  one place: 429 → `RateLimited` (with `Retry-After`), 5xx →
  `ServiceUnavailable`, 401/403 → `Auth`, other 4xx → `Provider`. `Retry-After`
  is parsed in both RFC 9110 forms; a date in the past means "retry now", and
  an unparseable value is ignored rather than treated as an instruction.
- **What changed — retry:** `RetryPolicy` (3 attempts, 500 ms initial, 30 s
  ceiling, ×2, equal jitter) applied by `LlmClient::with_retry`. Equal jitter
  rather than full jitter: full jitter can return a near-zero wait and hammer a
  provider that just asked for room. The concurrency permit is held across
  attempts — retries are one logical request, so they occupy one in-flight slot
  rather than re-queuing mid-sequence — and the token budget is charged once,
  for the attempt that actually consumed tokens.
- **One decision worth recording.** When a server asks for a longer backoff
  than `max_backoff`, this **stops** rather than clamping. Waiting it out would
  park an autonomous run for the interval the server named; retrying sooner
  would earn another 429 and burn an attempt. The error is returned with the
  server's own interval intact, so a layer that can decide to wait that long
  gets to make that call.
- **Behaviour change worth flagging:** `health_check` now requires a 2xx rather
  than merely a completed request, so a provider answering 401 no longer
  reports itself alive.
- **Residual (a) — only stream *establishment* is retried.** Once a chunk may
  have reached the caller, re-running the request would duplicate output rather
  than recover from it, so a mid-stream failure stays the caller's to handle.
- **Residual (b) — the budget is per-call.** A caller in a loop still issues
  many bounded sequences; there is no rate limiter above the retry loop.
- **Residual (c) — no circuit breaker.** A hard-down provider is re-probed, with
  a full backoff sequence, on every call. That is a separate finding.
- **Acceptance evidence:** 26 tests. The three status-classification ones run
  against a local socket serving raw HTTP 429/503/401, so the wiring is
  exercised rather than a hand-built `reqwest::Response` that would bypass the
  step that was missing. Verified negatively: forcing the policy to
  `disabled()` makes 5 of the client tests fail.

### P1-13 Make the agent loop consume the provider layer's retry outcome

- **Findings / invariants:** MED-009 (`PARTIALLY_FIXED`), INV-PROVIDER-3
  (`PARTIAL`)
- **Surface:** `soul-agent-core/src/lib.rs` — the strategy-level
  retry/replan/abort decision.
- **What already exists after P1-4:** `LlmError::is_retryable` is a single
  classification both layers can consult instead of maintaining divergent
  opinions, and `LlmError::RetriesExhausted { attempts, last }` distinguishes
  "failed once, transiently" from "the provider layer already backed off and
  retried N times".
- **Current risk:** `soul-agent-core` does not branch on either yet, and there
  is no attempt budget spanning both layers. A hard-down provider therefore
  still draws immediate strategy-level replanning on top of an already-exhausted
  provider budget — redundant work rather than a hot loop, since the provider
  layer now backs off, but still uncoordinated.
- **Acceptance tests:** a mock provider that always fails causes exactly one
  bounded provider sequence and then an abort rather than a replan; a mock that
  fails transiently is recovered by the provider layer without the agent loop
  observing a failure at all; the two layers' attempt counts sum to a
  configured ceiling rather than multiplying.

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
