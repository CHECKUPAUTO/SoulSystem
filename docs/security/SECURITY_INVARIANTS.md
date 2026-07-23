# SoulSystem Security Invariants

This document lists the invariants the production hardening effort must
establish and keep true. Each invariant is stated so that it can be checked by
a test or a reproducible command. An invariant is only considered **held** once
a test proves it; until then it is **target**.

Status legend: `HELD` (proven by a test in-tree) · `PARTIAL` (partially
enforced) · `TARGET` (not yet enforced).

The PR column refers to the [production hardening plan](PRODUCTION_HARDENING_PLAN.md)
PR sequence (A–P).

## Startup and mode

| ID | Invariant | PR | Status |
|----|-----------|----|--------|
| INV-ENV-1 | The runtime mode is decided only from an explicit `SOULSYSTEM_ENV` value; production is never inferred from the absence of a flag. | A | HELD (`soul-prod-guard`: `mode_unset_is_error`, `mode_unknown_is_error`) |
| INV-ENV-2 | In production, any single unmet security prerequisite aborts startup before any listener opens (fail closed). | A | HELD (`soul-prod-guard`: per-violation rejection tests) |
| INV-ENV-3 | No secret value ever appears in a guard violation, audit event, or startup error. | A | HELD (`violation_display_carries_no_secret_values`, `rejected_error_message_is_stable_and_secret_free`) |

## Tool dispatch and execution

| ID | Invariant | PR | Status |
|----|-----------|----|--------|
| INV-TOOL-1 | An unregistered tool name can only produce `UnknownTool`; it can never reach process execution. | B | HELD (`soul_tools`: `unknown_tool_never_executes`, `malformed_tool_names_rejected`) |
| INV-TOOL-2 | Every tool has a trusted, statically-registered capability that the caller cannot downgrade. | C | HELD (`soul_tools`: `write_tools_are_not_classified_as_read`, `capability_classification_is_trusted_and_correct`) |
| INV-EXEC-1 | No production-reachable code path invokes `std::process::Command` / `tokio::process::Command` / `libc::exec*` outside the single approved sandbox executor. | D | PARTIAL (`soul_tools::execute_shell` — the agent dispatch path, and the `execute_tool` helper that calls it — now route through `soul_sandbox`; `execute_shell_routes_through_sandbox` test. A repo-wide architecture ban on bare `Command` is PR D-2) |
| INV-EXEC-2 | Process tools fail closed when the mandatory OS isolation backend is unavailable in production. | A/D | PARTIAL (guard checks availability; enforcement in D) |
| INV-EXEC-3 | Process tools have no network access by default; egress is per-tool allowlisted. | D | TARGET |
| INV-EXEC-4 | Process output and CPU/memory/pids/fd/time are bounded; the process group is killed on timeout. | D | TARGET |

## Filesystem

| ID | Invariant | PR | Status |
|----|-----------|----|--------|
| INV-FS-1 | File tools cannot write outside their declared canonical root. | E | HELD (`soul_tools`: `outside_root_absolute_path_rejected`, `parent_traversal_rejected`, `absolute_path_within_root_is_allowed`) |
| INV-FS-2 | Symlink escape and check-then-open races are prevented. | E | HELD (`soul_tools`: `symlink_escape_of_existing_target_rejected`, `symlink_escape_of_directory_ancestor_rejected` — resolution canonicalises the nearest existing ancestor before the containment check) |
| INV-FS-3 | `.git`, config, and secret paths are protected from tool writes. | E | PARTIAL (`.git` protected — `dot_git_is_protected`; config/secret path protection is deployment-specific and deferred) |
| INV-FS-4 | All durable writes are atomic (temp + fsync + rename). | E/L | HELD for file-tool writes (`soul_tools::atomic_write` — `atomic_write_leaves_no_temp_file_and_is_readable_immediately`, `atomic_write_preserves_original_on_directory_failure`); other persistence backends (CCOS, journals) are PR L |

## Network services

| ID | Invariant | PR | Status |
|----|-----------|----|--------|
| INV-NET-1 | Every state-changing endpoint requires authentication and an authorization scope. | F | TARGET |
| INV-NET-2 | Binding a non-loopback address requires an active TLS serving path. | A/G | PARTIAL (guard rejects non-loopback-without-TLS in production; active TLS path in G) |
| INV-NET-3 | Webhooks fail closed when a secret is unset and reject invalid/replayed signatures. | F | TARGET |
| INV-NET-4 | Production CORS is an explicit origin allowlist, never permissive. | G | TARGET |
| INV-NET-5 | Request body, message, connection, and concurrency limits are enforced. | G | TARGET |

## Secrets

| ID | Invariant | PR | Status |
|----|-----------|----|--------|
| INV-SEC-1 | Secrets use redacting/zeroizing types; `Debug`/`Display` never reveal them. | J | TARGET |
| INV-SEC-2 | No secret appears in logs, metrics, traces, panic messages, or URLs. | J | PARTIAL (guard is secret-free; full sweep in J/N) |
| INV-SEC-3 | Default/example secrets are rejected in production. | A | HELD (`default_secret_rejected`) |

## Memory trust

| ID | Invariant | PR | Status |
|----|-----------|----|--------|
| INV-MEM-1 | Tool output is screened before it reaches any persistence path. | H | TARGET |
| INV-MEM-2 | Quarantined content never enters prompts or training. | H | TARGET |
| INV-MEM-3 | Every persisted memory record carries provenance and a trust level. | H | TARGET |
| INV-MEM-4 | Direct `store()` calls require a screened/trusted wrapper type. | H | TARGET |

## Planner and autonomy

| ID | Invariant | PR | Status |
|----|-----------|----|--------|
| INV-PLAN-1 | Planner history records the actual tool outcome (`success == actual result`). | I | TARGET |
| INV-PLAN-2 | Autonomous execution has hard budgets (turns, tool calls, writes, time, tokens). | I | TARGET |
| INV-PLAN-3 | Emergency stop denies new side effects and is durable across restart. | I | TARGET |

## Self-modification

| ID | Invariant | PR | Status |
|----|-----------|----|--------|
| INV-MOD-1 | Self-modification is disabled by default and cannot be enabled in production without a signing + approval policy. | A/K | PARTIAL (guard rejects unsigned self-mod in production; safe flow in K) |
| INV-MOD-2 | Self-modification never writes directly to live code; promotion is PR-only. | K | TARGET |

## Persistence

| ID | Invariant | PR | Status |
|----|-----------|----|--------|
| INV-PERSIST-1 | CCOS and other durable stores use atomic writes and detect corruption on load. | L | TARGET |
| INV-PERSIST-2 | A backup can be taken, state destroyed, and the backup restored with verified integrity. | L/P | TARGET |

## Truthful capabilities

| ID | Invariant | PR | Status |
|----|-----------|----|--------|
| INV-TRUTH-1 | Placeholder/simulated features never return a successful status in production. | O | TARGET |
| INV-TRUTH-2 | GPU fallback to CPU is explicit in runtime metadata and may be rejected in production. | O | TARGET |
| INV-TRUTH-3 | Health/readiness endpoints report truthful states; readiness fails when a required prerequisite is missing. | M | TARGET |

## CI and release

| ID | Invariant | PR | Status |
|----|-----------|----|--------|
| INV-CI-1 | `cargo deny` and `cargo audit` are required CI gates. | P | TARGET |
| INV-CI-2 | The toolchain is pinned; an MSRV job (1.85.0) is required. | P | TARGET |
| INV-CI-3 | Release artifacts include checksums, SBOM, signatures, and provenance. | P | TARGET |

---

Every `TARGET` invariant is tracked in [`findings.json`](findings.json) and mapped
to the PR that will make it `HELD`. No invariant may be downgraded from `HELD`
to `TARGET` without a corresponding test change and an explicit note here.
