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
through P1-4 plus P1-9's authentication half are closed**, but the verdict does not move to
`LIMITED_PRODUCTION` on that alone: the P1 set below still contains an
unbounded sandbox process count, no filesystem or PID isolation for sandboxed
commands, unbounded connection counts and no per-client rate limiting — and
per-scope authorization, while now implemented, is **opt-in**, so an
unconfigured deployment is still all-or-nothing.

**Every production listener now authenticates and fails closed** (`gateway`,
`ws_bridge`, `api`), which is the whole of INV-NET-1's authentication half.
Authorization is now *expressible* — one shared scope model, enforced per route
on both listeners (P1-9-B) — but it is **opt-in** by recorded product decision,
so a deployment that has not narrowed its tokens still hands full operator
power, including shell execution, to any authenticated caller. INV-NET-1 stays
`PARTIAL` for that reason: the mechanism existing is not the property holding.

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

- **Findings / invariants:** INV-NET-4 (**now `HELD`** for every workspace
  member — the other listeners were closed by P1-10),
  INV-NET-5 (**now `HELD`** — the three residuals recorded below were closed by
  P1-11)
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
- **Residual (a) — no connection bound.** ~~The concurrency layer limits work in
  flight, not accepted connections or queued requests, so a slow-loris style
  connection flood is not addressed.~~ **Closed by P1-11:** connections are
  refused past a cap rather than parked, on both the gateway and
  `src/ws_bridge.rs`.
- **Residual (b) — no per-client rate limiting** on `soul_gateway`.
  ~~`soul-dashboard` has a `SimpleRateLimiter`; the gateway has none.~~
  **Closed by P1-11:** a per-principal token bucket, charged after
  authentication.
- **Residual (c) — the other listeners are untouched.** ~~`src/api.rs` and
  `src/ws_bridge.rs` carry no body, message or concurrency limits at all.~~
  **Closed by P1-11.**
- **Residual (d) — four other crates still call `CorsLayer::permissive()`:**
  `soullink-gateway` (`src/cli/run.rs`), `soullink-inference`
  (`src/bin/turboquant-proxy.rs`), `soullink-orchestrator-v3` (`src/main.rs`)
  and `soulsystem-lite` (`src/main.rs`). Each was checked against
  `cargo tree -p soulsystem --edges normal`; none is in the production binary's
  dependency graph, so none is reachable from `soulsystem`. They are still real
  services if deployed on their own. Tracked as P1-10, **now closed** — which
  also found three more the list above missed: `brain-system-rs`, and
  `soullink-proxy` and `synergie` spelling it `.allow_origin(Any)`.

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

### ~~P1-9 Authenticate `src/api.rs`~~ — CLOSED (authentication only)

- **Findings / invariants:** CRIT-007 (residual), INV-NET-1
- **Status:** closed by `security/p1-9-api-auth-and-scopes` **for
  authentication**. Per-scope authorization is *not* done and is split out as
  P1-9-B below, because it depends on a product decision I should not make
  unilaterally.
- **What changed:** `src/api.rs` applies its own fail-closed bearer middleware
  (`ApiAuth`, `SOULSYSTEM_API_TOKEN`) to every route except `/health`. That
  listener exposed `/api/exec` — shell execution via `BoundSystem` — and
  `/api/pty/*` — interactive terminals — with **no authentication of any kind**,
  mitigated only by its `127.0.0.1:9023` bind. Loopback is not a control here:
  it does not stop another user on the host, nor a request driven from a page
  the operator's browser loaded.
- **`/metrics` is inside the authenticated set.** Request counts and error
  rates describe what the host is doing, so it is a disclosure route and gets
  the same treatment `/v1/status` got on the gateway. A scraper now needs the
  token.
- **Its own variable, not the gateway's.** These are different listeners with
  different audiences; sharing one value would mean rotating one forces
  rotating the other.
- **Behaviour change worth flagging.** `prod_guard::assemble_posture` used to
  hardcode this listener's posture to `false`. That made the
  unauthenticated-listener violation *unconditional*, so **production startup
  could never succeed** regardless of configuration. The posture is now derived
  from the real `ApiAuth`, so a correctly configured production deployment can
  actually start — and an unconfigured one still aborts.
  (`api_posture_is_derived_from_its_real_configuration`,
  `production_rejects_an_unauthenticated_api_listener`.)
- **Acceptance evidence:** 12 tests on the listener plus 2 on the guard.
  Verified negatively: removing the `route_layer` makes 5 of them fail.

### ~~P1-9-B Per-scope authorization~~ — CLOSED (mechanism; default is opt-in)

- **Findings / invariants:** CRIT-007 (residual), INV-NET-1 (still `PARTIAL`)
- **Status:** closed by `security/p1-9b-per-scope-authorization`.
- **Product decision, recorded:** the maintainer chose **"grant everything to an
  unscoped credential"** — scopes are **opt-in**. Upgrading cannot start
  returning 403 to automation that worked yesterday. The honest consequence is
  stated below rather than buried: this makes least privilege *expressible*, not
  *default*.
- **One model, both listeners.** `soul_gateway::scope` defines
  `Scope::{Read,Write,Exec,Admin}` and `ScopeSet`; the binary's `src/api.rs`
  consumes the same types. Two enums would have moved the coherence problem
  rather than solved it.
- **Scopes do not imply one another** apart from `Admin`. Implication is easy to
  get subtly wrong and hard to audit, so a credential that should read *and*
  write says so. `Exec` is separated from `Write` specifically because it is the
  scope that turns a leaked token into host compromise.
- **Declared, not checked ad hoc.** The requirement is a `route_layer` on a
  group of routes, so it lives next to the route. Each listener additionally has
  a guard test that **reads its own source**, extracts every declared route, and
  fails if one sits outside every scoped group. That guard caught a real gap
  (`/v1/stream`) on its first run rather than after review.
- **403, not 401,** for insufficient scope — 401 would invite a retry with
  credentials that fail identically. Authentication still runs first, so an
  anonymous caller cannot learn which scope a route needs.
- **Configuration:** `SOULSYSTEM_GATEWAY_TOKENS` gains
  `principal:scope1+scope2=token`; the api listener gains
  `SOULSYSTEM_API_SCOPES`. An unrecognised scope name is dropped and logged
  rather than granted — a typo like `exce` must not become `exec` — and a
  trailing colon (`alice:`) grants nothing rather than silently restoring
  everything.
- **Why INV-NET-1 stays `PARTIAL`.** The mechanism existing is not the property
  holding. A deployment that does not configure scopes is exactly as exposed as
  it was before, and one compromised unscoped token still yields full operator
  power. Only an operator can move this, and this repository cannot prove they
  have.
- **Optional follow-up, not scheduled:** have the production startup guard
  report — or refuse — a credential holding `Admin`, converting the opt-in
  default into an explicit per-deployment acknowledgement. That is another
  product decision, so it is noted rather than assumed.

### ~~P1-8 Widen the process-execution guard beyond the binary crate~~ — CLOSED (guard); 12 sites remain as P1-8-B

- **Findings / invariants:** CRIT-001 (residual), HIGH-002, INV-EXEC-1 (stays
  `PARTIAL`)
- **Status:** closed by `security/p1-8-widen-process-execution-guard`.
- **Every workspace member is now scanned**, not just the binary. Member
  directories are parsed from the root `Cargo.toml` rather than hardcoded, so
  adding a crate to the workspace brings it under the guard automatically — a
  new crate should not have to be *remembered* into a security scan.
- **All 30 production spawn files are classified**, each with a category and a
  written reason:

  | Category | Files | What it means |
  |---|---|---|
  | `sandbox-implementation` | 5 | The crate *is* the isolation boundary. Forbidding its `Command` would forbid the sandbox. |
  | `host-control-fixed-argv` | 8 | `systemctl`, `nvidia-smi`, `ss`, `df`, `tmux`, `docker`, `which`, `stty`. No caller-controlled argv. |
  | `dev-tooling` | 4 | `cargo` and `git` in self-modification and workflow paths. Host-level by design. |
  | `unsandboxed-arbitrary-command` | 12 | **Known problems, recorded — not approvals.** |
  | `no-reachable-caller` | 1 | Present, no non-test caller (LOW-005). |

- **The allowlist is a record, not an endorsement.** The 12
  `unsandboxed-arbitrary-command` entries are the actual finding of this work:
  `sh -c` from workflow nodes and conditions, the brain's shell action,
  soul-kernel's action and perception paths, soul_automation, both soul-bridge
  child spawns, two `python3` orchestrator spawns, soul-kernel/parallel, and the
  gateway's `osascript` iMessage provider. They are listed so they are visible
  and countable, not so they are blessed.
- **A budget pins the category both ways.** It fails if the count grows, and it
  *also* fails if sites are fixed without lowering the budget — otherwise the
  ratchet would quietly stop constraining anything.
- **One false positive is exempted, carefully.**
  `soul_sandbox/src/seccomp.rs` matches `execve` only because it names
  `SYS_execve` as the syscall it **blocks**. Forcing an allowlist entry there
  would misrepresent the one file most clearly doing the right thing — so it is
  exempted, and a test asserts the exemption stays sound by requiring the filter
  to still name `SYS_execve`.
- **Verified negatively three ways:** an introduced `Command` in a
  non-allowlisted crate is caught; breaking the member parsing fails two
  vacuity guards rather than passing silently; a stale allowlist entry is
  caught.
- **Why INV-EXEC-1 stays `PARTIAL`.** A guard that *records* 12 unsandboxed
  arbitrary-execution sites has not removed them. The invariant holds when they
  are gone, not when they are inventoried.

### P1-8-B Route the remaining 5 sites through the sandbox, and decide about the non-member trees

- **Findings / invariants:** CRIT-001 (residual), INV-EXEC-1
- **Done:** round 1 the request-reachable spawn route; round 2 the `sh -c`
  group; round 3 soul-bridge's two MCP child spawns.
  `UNSANDBOXED_ARBITRARY_BUDGET` is **5**.
- **What is left, still not homogeneous.**
  - `soul-kernel`'s `OptimizeSystem` runs three *fixed* privileged `sh -c`
    strings (`sync && echo 3 > /proc/sys/vm/drop_caches`, `journalctl
    --vacuum-time`, `find /tmp -delete`). The sandbox would refuse all three —
    they are shell composition and destructive-pattern matches. No caller input
    reaches them, so there is nothing to inject; they want **rewriting as direct
    operations**, not wrapping. Doing that is a behaviour question about what
    those operations should be, not a mechanical migration.
  - `soul-kernel/perception` is mostly fixed argv (`df --output=pcent /`,
    `systemctl is-active`) plus one fixed `journalctl | grep` pipeline — the
    pipeline is the only part the sandbox cannot take as-is.
  - `soul_automation` and `soullink-orchestrator`'s `main` need reading before
    they can be classified.
  - `soul_gateway`'s `osascript` iMessage provider is **macOS-only** while the
    isolation hook is Linux-specific. Sandboxing it on Linux would confine code
    that never runs there and do nothing on the platform where it does. Needs a
    decision, not a migration.
- **Decide what to do about the non-member trees.** `intel-integrations/`,
  `openevolve/`, `backlog/`, `os-agents/`, `openclaw-evolution/`, `soul-rsi/`,
  `jit-agentic-engine/`, `soullink-node/`, `turboquant/` and
  `scirust-chronos-agent/` hold a further **112 spawn sites** and are not
  workspace members, so `cargo build --workspace` never builds them and this
  guard says nothing about them. Either they are shipped code — in which case
  they belong in the workspace and under the guard — or they are dead weight
  that should be deleted. A product question about what this repository ships.

### ~~P1-8-B(3) The structured-argv group~~ — CLOSED

- **Findings / invariants:** CRIT-001 (residual), INV-EXEC-1
- **soul-bridge's two MCP child spawns** (`ccos.rs`, `octasoma.rs`) now build
  their command through `Sandbox::supervised_command`. Config-supplied values
  (`workspace`, `store`, `ollama_url`, `ollama_model`) go through
  `SpawnSpec::value`, so a config entry shaped like `--something` is refused
  rather than becoming a flag to the spawned binary.
- **A recategorisation, stated as such.** `soul-kernel/parallel` was labelled
  `unsandboxed-arbitrary-command`, but `run_action` matches its argument against
  exactly two string literals and spawns `sync` or a fixed `systemctl
  list-units`. No caller input reaches argv. `host-control-fixed-argv` is what
  the rest of the allowlist already calls this shape. **The budget drops by one
  for that entry because the label was wrong, not because anything was fixed** —
  worth saying plainly, since a falling number otherwise reads as progress.
- **Two more gaps in `spawn_supervised`, both found by real callers.**
  - `SpawnSpec::piped_stdio`. The API assumed a fire-and-forget daemon and
    nulled stdio. These children *are* their pipes — MCP servers exchanging
    JSON-RPC on stdin/stdout. Null stdio would not have failed at spawn: the
    child starts, reads EOF, and the parent waits forever for a reply on a pipe
    that was never created. A hang, not an error, which is the worse failure.
  - `Sandbox::supervised_command`. Both callers do async I/O on those pipes, so
    a `std::process::Child` would have put blocking reads on a tokio task and
    stalled a runtime worker for the length of every call. Returning the built
    but unspawned `Command` lets the caller do
    `tokio::process::Command::from(cmd)` and keep async pipes, while validation
    and the `pre_exec` hook still happen in one place. `soul_sandbox` keeps no
    `tokio` dependency.

### ~~P1-8-B(2) The `sh -c` group~~ — CLOSED

- **Findings / invariants:** MED-014 (new), CRIT-001 (residual), INV-EXEC-1
- **Four sites, three files off the allowlist.** soullink-workflow's bash node
  and `TestsPass` condition, soullink-actions' shell tool, and soul-kernel's
  `Action::ExecuteShell` now go through `Sandbox::execute`. Unlike the spawn
  route, `execute` *is* the right API here: these pass a shell string, which is
  exactly what it takes.
- **A real bypass in existing "security" code (MED-014).**
  `is_safe_shell_command` blocked `;`, `&&`, `||`, `|`, backtick, `$(`, `${`
  and `>` — but not a **newline**, which `sh` treats as a separator exactly like
  `;`. `"ls\nrm -rf /tmp/x"` passed validation and both halves ran. Verified
  directly. `&`, `<` and `$'\x3b'`-style encodings were missed too. The fix is
  not a longer list — the next missing character reopens it — but removing the
  shell. The filter stays as a first pass, with a test asserting the newline is
  **still admitted**, so anyone who later "fixes" the list has to read why the
  list was never what held.
- **This changes what these sites can express.** Pipelines, redirects and `&&`
  chains no longer work; the sandbox neutralises them, which is the whole
  point. A workflow relying on `a | b` now fails instead of doing something
  else quietly — the right failure direction, but a behaviour change for
  existing definitions, not transparent hardening.
- **Two gaps in the sandbox surfaced and were closed.**
  `SandboxPolicy::working_dir` — soullink-actions' shell tool had a `workdir`
  the sandbox could not express, and silently dropping it would have moved
  commands to a different directory without failing. And `SandboxVerdict::timed_out`
  — a deadline kill was previously reported *only* by appending a sentence to
  `stderr`, so a caller distinguishing "timed out" from "exited without a code"
  had to string-match the sandbox's own prose.

### ~~P1-8-B(1) The request-reachable spawn route~~ — CLOSED

- **Findings / invariants:** MED-013 (new), CRIT-001 (residual), INV-EXEC-1
- **The roadmap's own instruction was wrong for this site, and following it
  would have made things worse.** "Route it through soul_sandbox" means
  `Sandbox::execute`, which (a) waits for the process to exit and kills its
  group at `policy.timeout` — fatal for a daemon that is supposed to stay up —
  and (b) takes a `&str` and splits it on whitespace. The route already built
  argv from discrete `.arg()` calls with no shell, so converting to a string
  command would have *introduced* a splitting surface that did not exist: a
  domain containing a space becomes two arguments. The checkbox would have been
  ticked and the code would have been less safe.
- **So the sandbox gained the API it was missing.** `SpawnSpec` +
  `Sandbox::spawn_supervised`: structured argv that is never joined-then-split,
  the same `setpgid`/`setrlimit`/seccomp `pre_exec` hook as every other spawn
  path, and no waiting. `SpawnSpec` distinguishes `flag()` (the program's own)
  from `value()` (came from outside) and refuses a value beginning with `-`,
  which makes argument injection unrepresentable rather than merely discouraged.
- **The real defect was argument injection, not command injection.** `domain`
  was the value of `--brain`; a domain of `--config=/etc/shadow` is a flag to
  `brain_v12.py`'s parser, not a value. Also fixed: unbounded spawning (no cap,
  nothing reaped — now 16 max, with failed health checks killing the process
  group), a `u16` port-search overflow, and an `.unwrap()` that would have
  killed the supervising task and orphaned the child.
- **`network_isolated` is false here, deliberately and visibly.** A brain binds
  a port and must be reachable; in the default empty netns it would bind a port
  nothing can reach. Pinned by a test so a later defaults change confronts it.
- **What is NOT fixed: the route has no authentication.** CORS is not auth and
  stops nothing that is not a browser. Recorded as MED-013 with the cap
  described as turning an unbounded denial of service into a bounded one — not
  as making the route safe to expose.

### ~~P1-5 Secret-type sweep beyond `soullink-secrets`~~ — CLOSED (type + guard); 16 structs remain as P1-5-B

- **Findings / invariants:** HIGH-001, INV-SEC-1, INV-SEC-2 (still `PARTIAL`)
- **Status:** closed by `security/p1-5-secret-type-sweep`.
- **The type.** `soulsystem_common::secrets::SecretString` — `Debug`, `Display`
  **and** `Serialize` all render a fixed redaction. Nobody writes
  `println!("{}", token)` on purpose; it leaks through the derive, through a
  panic message that formats the enclosing struct, or through a debug endpoint
  that serializes it. This removes the *accidental* path, which is the one that
  actually happens.
- **Two decisions worth stating.** The redaction is a **fixed string, not a
  length-derived mask** — `***` versus `****************` discloses the length
  and narrows a brute-force search. And `Serialize` writes the redaction too: a
  config struct is routinely serialized to a debug endpoint or a state dump, and
  round-tripping the plaintext through those is the same leak in a different
  coat.
- **Five structs migrated:** `WsBridgeConfig::shared_secret`,
  `LlmConfig::auth_token`, `OpenclawConfig::auth_token`,
  `DiscordWebhookBody::token` (an inbound webhook body is exactly what gets
  logged when parsing fails), and — the one that matters most —
  **soullink-security's scanner `Finding::secret`**, which derived `Debug` *and*
  `Serialize` over the very credential it had just detected, next to a `masked`
  field that existed precisely because someone already knew displaying it was
  unsafe. The derive defeated their own mitigation.
- **A workspace-wide guard** fails on any new secret-named `String` field on a
  `Debug`-deriving struct, with the 16 unmigrated structs recorded and a
  two-way count budget.
- **A parser gap I found and fixed rather than shipped.** The first version of
  the guard silently missed one-line structs
  (`pub struct P { pub api_key: String, .. }`) and enum variants with inline
  braces (`Basic { username: String, password: String }`) — three real
  `Debug`-deriving structs among them. A guard that quietly skips a case is
  worse than no guard: it reports success over something it never examined. The
  scan now reads whole lines, and the shapes it used to miss are unit-tested.
- **Two limitations stated, not counted as clean.** The guard keys on
  `#[derive(Debug)]`, so a credential-holding struct deriving only `Clone` —
  `clawd::Settings` and `soul-dashboard::AppState` both do — is not reported.
  That is a smaller exposure, not an absent one: it still leaks via a
  hand-written `Display`, via serde, or the moment somebody adds `Debug`. And
  the match is **name-based**, so a credential in a field called `value` or
  `blob` will not be found by any amount of this.
- **Why INV-SEC-2 stays `PARTIAL`.** 16 structs still hold plaintext
  credentials on `Debug`-deriving types. A budget that stops the set growing is
  not the same as the set being empty.

### ~~P1-5-B Migrate the 16 recorded structs~~ — CLOSED

- **Findings / invariants:** INV-SEC-2 (now `HELD` for the `Debug` set)
- **Status:** closed by `security/p1-5b-migrate-recorded-secret-structs`.
  `NOT_YET_MIGRATED_BUDGET` is `0`.
- **Two of the sixteen needed a different type.** `HelloOk::device_token` and
  `AuthInfo::token` are handshake fields: serialization is their purpose. A
  redacting `Serialize` would have handed every client `<redacted>` as its
  token — authentication broken at runtime, nothing failing to compile. They
  use the new `ProtocolSecret` instead, which redacts `Debug`/`Display` and
  serializes faithfully.
- **The name-based blind spot is real and was hit.** `synergie::Telegram::bot`
  holds a bot token in a field called `bot`; no amount of matching on `token`,
  `secret` or `api_key` finds it. It was migrated because a call site led there,
  not because the guard reported it.

### P1-5-C Beyond the `Debug` set

- **Findings / invariants:** INV-SEC-2 (residual)
- **Current risk:** the guard keys on `#[derive(Debug)]` and on field *names*.
  `clawd::Settings` and `soul-dashboard::AppState` hold credentials while
  deriving only `Clone` — a smaller exposure, not an absent one. A credential in
  a field named `value`, `blob` or `bot` is invisible to it.
- **Acceptance tests:** the guard reports `Clone`-only credential holders; a
  check that does not depend on field naming (for example, flagging any
  `String` field whose value flows into an `Authorization` header).

### P1-6 Memory provenance and trust metadata — DONE (INV-MEM-4 held; INV-MEM-3 partial)

- **Findings / invariants:** INV-MEM-2 (`PARTIAL`), INV-MEM-3 (`PARTIAL`),
  INV-MEM-4 (`HELD`), MED-011 (`FIXED_AND_VERIFIED`)
- **What landed:** `soul-agent-core::provenance` (`TrustLevel`, `MemorySource`,
  `MemoryStore`, `MemoryProvenance`, `ProvenanceLog`); the trust level moved
  *onto* `ScreenedContent` so a persist call site cannot hold the content
  without it; `ccos`/`semantic` made private and `observe_untrusted` made the
  only public write path; quarantined content refused rather than written.
- **What P1-6 uncovered:** ingesting the quarantine placeholder into the causal
  graph did not store a redaction — it erased the file's record, because
  `ingest_source` replaces a node's contents and the placeholder parses to zero
  symbols. Recorded as MED-011 and fixed here.
- **Both acceptance tests met**, with the scope limits recorded in the
  invariant register rather than counted as clean.

### ~~P1-6-B Durable memory provenance~~ — CLOSED for durability; record shapes remain

- **Findings / invariants:** INV-MEM-3 (still `PARTIAL`), MED-015 (new)
- **Both acceptance tests met.** `a_trust_level_survives_a_restart` and
  `an_evicted_record_is_distinguishable_from_an_unscreened_one`, the second
  driven through the real eviction path rather than by reaching into the struct.
- **Two defects found in P1-6's own code (MED-015).**
  1. **The documented bound bounded nothing.** `ProvenanceLog` said its ring was
     bounded "so a long autonomous run cannot grow it without limit" — and the
     ring was. The `latest` index beside it was not: `record` inserted and
     nothing ever removed. Now capped at `INDEX_CAPACITY`, oldest first.
  2. **`Option` conflated two opposite answers.** `latest_for` returned `None`
     both for "screened, and the note aged out" and for "never screened". Those
     call for opposite decisions, and treating the first as the second is
     INV-MEM-3's own failure mode in miniature. `lookup` now returns
     `Known` / `TrustOnly` / `Unknown`; an evicted record keeps its
     `TrustLevel`, which is the field a caller actually acts on.
- **A corrupt index is an error, not an empty start.** Starting empty would turn
  "the provenance store is broken" into "nothing was ever screened" — arriving
  at exactly the state the invariant exists to prevent, by way of a failure
  nobody saw. Same for a version mismatch.
- **What is honestly still open**, and why this does not close INV-MEM-3:
  - The **record shapes are unchanged.** The roadmap asked for the stores to
    carry a trust field; this makes provenance durable *alongside* the records,
    not part of them. A write that never calls `record_provenance` is still
    invisible.
  - **Nothing calls `load_memory_provenance` at startup yet.** The API exists
    and is tested; wiring it into each binary entry point is follow-on.
  - `persist` is caller-driven, not per-record: a write per observation would
    put an fsync-shaped cost on an autonomous loop's hot path. Records since the
    last call are lost on a crash and return to `Unknown`.
  - Past a second cap a record is genuinely forgotten and `Unknown` goes
    inconclusive again. `forgotten_count()` makes that visible rather than
    hiding it, but a caller that ignores it can still be misled.
  - `soul-memory`, `soullink-memory` and `soullink-memory-hierarchy` have their
    own write paths and remain outside the P1-6 guard entirely.
- **Tracked as MED-015-B.**

### P1-7 Transactional multi-file persistence and backup/restore qualification — DONE (INV-PERSIST-2 held; INV-PERSIST-1 partial)

- **Findings / invariants:** HIGH-005 (`FIXED_AND_VERIFIED`), INV-PERSIST-1
  (`PARTIAL`), INV-PERSIST-2 (`HELD` for the CCOS state directory)
- **What landed:** a `state.manifest` written **last**, carrying a format
  version, a generation counter and a SHA-256 digest per member file;
  `verify_set_integrity`; `restore_runtime` checking the set before
  deserializing; `restore_runtime_strict`; and `backup_to` / `restore_from`
  that verify before acting.
- **Both acceptance tests met**: `a_torn_multi_file_write_is_detected` and
  `backup_destroy_restore_recovers_verified_state` (save → back up → delete the
  live directory → restore → verify set *and* reloaded contents).

### ~~P1-7-B Repair from the previous generation~~ — CLOSED; two gaps become P1-7-C

- **Findings / invariants:** INV-PERSIST-1 (`PARTIAL`), HIGH-005
- **Acceptance test 1 met:** `a_torn_set_is_repaired_from_the_previous_generation`.
  `save_state` snapshots the outgoing **verified** generation into
  `state.prev` before overwriting anything, and `restore_runtime_repairing`
  falls back to it when the live set is torn.
- **It is recovery, not reconstruction, and the API says so.** The interrupted
  generation is gone; *n-1* takes its place, so **a repair loses the most recent
  save**. That is why it is a separate call rather than something
  `restore_runtime` does silently — a caller who would rather investigate a tear
  than lose a generation has to be able to choose, and `restore_runtime` still
  refuses a torn set exactly as before.
- **Only a verified set is snapshotted.** A save that finds the live set already
  torn leaves the existing fallback alone: the older good generation is worth
  more than the newer broken one. Pinned by
  `saving_over_a_torn_set_does_not_destroy_the_good_fallback`.
- **The snapshot is verified in its own right**, not merely copied from
  something that verified at the time, and staged through a temp directory so an
  interrupted snapshot cannot leave a half-copied fallback. A fallback that is
  itself torn is worse than none — it looks available and fails when needed.
- **A snapshot failure does not fail the save.** Refusing to persist new state
  because the *backup of the old state* could not be written would trade a real
  loss for a hypothetical one; it warns and continues.
- **Cost, stated:** the state directory roughly doubles.

### P1-7-C The two gaps P1-7-B did not close

- **Findings / invariants:** INV-PERSIST-1 (`PARTIAL`)
- **Strict restore is still opt-in**, so a *deleted* manifest remains
  indistinguishable from one that never existed, and anyone who can unlink a
  file in the state directory can still opt it out of verification.
  `restore_runtime_strict` refuses that, but no production caller uses it.
  **Making it the default needs a migration window — a deployment decision, so
  it is being asked rather than assumed.**
- **Backup qualification still covers only the CCOS three-file state
  directory.** The provenance index added by P1-6-B is now a second durable
  store with no backup/restore test, and `soul-memory` / `soullink-memory` /
  `soullink-memory-hierarchy` have their own persistence with none either.
- **Acceptance tests:** strict restore is the default with a documented
  migration path; a backup/restore qualification covering the provenance index
  and at least one of the memory stores.

### ~~P1-10 Permissive CORS in the non-production HTTP services~~ — CLOSED

- **Findings / invariants:** INV-NET-4 (now `HELD` for every workspace member)
- **Status:** closed by `security/p1-10-shared-cors-policy`.
- **It was seven services, not four.** The roadmap listed four. `brain-system-rs`
  was simply missed. `soullink-proxy` and `synergie` spelled the same defect as
  `.allow_origin(Any)` rather than `CorsLayer::permissive()`, so a search for
  the latter never saw them — which is why the guard matches all three
  spellings and why the count is now enforced rather than remembered.
- **`synergie` was the worst case.** `cors_allow_any` defaulted to `true` in
  both its serde attribute and its `Default` impl, so an operator who omitted
  the field served `Access-Control-Allow-Origin: *` from an API whose
  `auth_token` also defaults to `None`. The field is still parsed — so an
  existing config is not silently reinterpreted — but it is no longer honoured
  and logs a warning pointing at `cors_origins`.
- **One policy, not a seventh copy.** The roadmap's own instruction: two copies
  is where duplication is still cheaper than a dependency edge, three is not.
  `crates/soul-cors` deliberately does not depend on `axum`, so services on
  axum 0.7 and 0.8 share it. Allowed methods stay a per-service parameter,
  because a read-only listener has no business advertising `POST`.
- **`Disabled` still installs a layer.** A layer emitting no
  `Access-Control-Allow-Origin` is what makes a browser refuse the response;
  removing the layer would not be equivalent.
- **Remaining residual:** the guard covers workspace members only. The
  non-member `os-agents/soul_gateway` still calls `permissive()` (LOW-003), and
  an allowlist assembled at runtime from an attacker-influenced source is not
  visible to source scanning. `soul_gateway` and `soul-dashboard` kept their own
  P1-2 policies at the time — both were already fail-closed and tested, and
  rewriting hardened, production-reachable code to remove duplication is a
  refactor, not a security fix. **P1-10-B has since folded them in** (below), so
  one implementation now decides which origins are allowed.

### ~~P1-10-B Fold the two P1-2 CORS policies into `soul-cors`~~ — CLOSED

- **Findings / invariants:** INV-NET-4
- **Not a vulnerability fix, and not recorded as one.** Both copies were
  fail-closed and tested. What they were was *two* implementations of one rule
  — and a rule that fails open when it drifts. An allowlist still admitting an
  origin that was supposed to be revoked looks exactly like an allowlist that is
  working, so the copy a future fix misses is the copy nobody notices.
- **`soul_gateway`'s `CorsPolicy` was byte-identical** to `soul_cors`'s — same
  `Disabled`/`Allowlist` shape, same parse, same `allows`, same layer. Replaced
  with `pub use soul_cors::CorsPolicy`, which keeps `limits::CorsPolicy` naming
  at all ~20 call sites, so no API break. 74 lines removed.
- **The dashboard's copy was *not* identical, and that was the risk.** It
  advertised `GET` alone; `soul_cors` offers both `read_only_layer` (`GET`) and
  `read_write_layer` (`GET`/`POST`/`OPTIONS`). Folding a read-only listener onto
  the read-write layer would have widened its policy while reading as pure
  deduplication — which is the failure mode this kind of refactor actually has.
  It uses `read_only_layer`, and a doc comment says why.
- **One behaviour does change.** The dashboard's preflight responses now carry
  `Access-Control-Max-Age: 600`, which its hand-rolled layer omitted. It does
  not widen *which* origins are allowed, but a browser may cache a preflight
  decision for ten minutes, so revoking an origin can take that long to be
  observed by a browser that already asked.
- **A guard so a fourth copy cannot appear.**
  `only_soul_cors_decides_which_origins_are_allowed` fails on any `.allow_origin(`
  outside `crates/soul-cors`. Verified negatively: re-introducing a hand-rolled
  layer in the dashboard fails the test naming the file.

### ~~P1-11 Connection bounds, rate limiting, and limits on the other listeners~~ — CLOSED

- **Findings / invariants:** INV-NET-5 (now `HELD`, with one stated residual)
- **Status:** closed by `security/p1-11-connection-bounds-rate-limiting`.
- **Connections are refused, not parked.** `ConnectionLimiter::try_acquire`
  returns `None` immediately at the cap and the socket is dropped. Waiting for a
  slot is exactly what a slow-loris flood wants — their one socket would cost us
  a socket *and* a queue entry. Refusing costs them a reconnect.
- **This required dropping `axum::serve`** on the plain-HTTP path, because it
  accepts every connection the OS offers and there is no layer that can refuse
  one — a tower layer sees requests, and a connection that never completes a
  request never becomes one. Both listeners now run a bounded accept loop, and
  both carry a guard test that fails if anyone reverts to `axum::serve`.
- **Rate limiting is keyed by principal, charged after authentication.** Keyed
  by principal so a token cannot escape its limit by reconnecting from a new
  port. Charged *after* authentication so an anonymous flood cannot exhaust a
  real principal's allowance — otherwise the limiter becomes the attack. That
  ordering is tested directly, not just asserted in a comment.
- **Idle buckets are evicted.** Without that, an IP-keyed limiter on a public
  listener grows once per distinct key seen forever: a memory-growth bug wearing
  a rate limiter's clothes.
- **The other two listeners** gain what they had none of: `src/api.rs` gets
  `DefaultBodyLimit` and a bounded accept loop; `src/ws_bridge.rs` gets a
  connection cap and `max_message_size`/`max_frame_size` applied *at handshake
  time*, so an oversized frame is refused by the protocol layer rather than
  buffered and then checked.
- **Corrected while writing this.** Two claims I had to fix rather than ship:
  a burst smaller than the sustained rate does **not** make that rate
  unreachable (it only stops the allowance being spent instantaneously), so the
  clamp that raised it was removed; and the api listener's body limit does
  **not** apply before authentication — the auth layer rejects first, without
  reading the body, which is a better outcome than the one the original comment
  described.
- **Remaining residual:** the gateway's rate limit is per *principal* only. An
  unauthenticated flood is bounded by the connection cap rather than by a
  per-IP budget; adding one needs `ConnectInfo` wired through the router, which
  is not done here.

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
