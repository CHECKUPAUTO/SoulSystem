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
| INV-NET-1 | Every state-changing endpoint requires authentication and an authorization scope. | F | PARTIAL (`soul_gateway`: mandatory bearer auth on all `/v1/*` operator routes incl. read/status/disclosure routes, fail-closed when unconfigured — `operator_route_rejects_*`, `state_changing_operator_route_also_requires_auth`, `goals_and_events_disclosure_routes_require_auth`; distinct per-scope authorization, `src/api.rs`, and MCP/PTY endpoints remain TARGET) |
| INV-NET-2 | Binding a non-loopback address requires an active TLS serving path. | A/G | PARTIAL (guard rejects non-loopback-without-TLS in production; active TLS path in G) |
| INV-NET-3 | Webhooks fail closed when a secret is unset and reject invalid/replayed signatures. | F | PARTIAL (fail-closed on unset secret HELD — `soul_gateway`: `discord_webhook_fails_closed_when_secret_unset`, `slack_webhook_fails_closed_when_secret_unset`, `whatsapp_webhook_fails_closed_when_secret_unset`; cryptographic signature verification and replay protection remain TARGET, follow-up PR) |
| INV-NET-4 | Production CORS is an explicit origin allowlist, never permissive. | G | TARGET |
| INV-NET-5 | Request body, message, connection, and concurrency limits are enforced. | G | TARGET |

## Secrets

| ID | Invariant | PR | Status |
|----|-----------|----|--------|
| INV-SEC-1 | Secrets use redacting/zeroizing types; `Debug`/`Display` never reveal them. | J | HELD for `soullink-secrets` (`SecretsCrypto::master_key` is `secrecy::Secret<Vec<u8>>`; derived keys and decrypted plaintext are `zeroize::Zeroizing<Vec<u8>>`; `SecretValue`/`DecryptedSecret` redact `Debug` and drop `Clone` — `zeroize_actually_clears_key_bytes`, `secret_value_debug_redacts_bytes`, `decrypted_secret_debug_does_not_leak_value`). Not yet extended to other secret-holding code (LLM API keys, webhook secrets in `soul_gateway`) — follow-up. |
| INV-SEC-2 | No secret appears in logs, metrics, traces, panic messages, or URLs. | J | PARTIAL (guard and `soullink-secrets` are secret-free/redacted; full workspace sweep is a follow-up) |
| INV-SEC-3 | Default/example secrets are rejected in production. | A | HELD (`default_secret_rejected`) |

## Memory trust

| ID | Invariant | PR | Status |
|----|-----------|----|--------|
| INV-MEM-1 | Tool output is screened before it reaches any persistence path. | H | HELD for the agent tool-dispatch path (`soul-agent-core`: `screen_tool_output` now runs before `ccos_observe_tool`/`planner.history.record`; `ccos_never_ingests_unscreened_malicious_payload`) |
| INV-MEM-2 | Quarantined content never enters prompts or training. | H | PARTIAL (quarantine placeholder — not the raw payload — is what reaches CCOS/planner/chat session; a dedicated non-injectable quarantine store with retention/deletion policy is a follow-up) |
| INV-MEM-3 | Every persisted memory record carries provenance and a trust level. | H | TARGET (full `MemoryProvenance` struct not yet introduced; follow-up) |
| INV-MEM-4 | Direct `store()` calls require a screened/trusted wrapper type. | H | HELD for `ccos_observe_tool` (`soul-agent-core::screening::ScreenedContent` — private constructor, only obtainable via `screening::screen`; `ccos_observe_tool`'s `output` parameter is `&ScreenedContent`, not `&str`, so it cannot be called with unscreened data from anywhere in the crate) |

## Planner and autonomy

| ID | Invariant | PR | Status |
|----|-----------|----|--------|
| INV-PLAN-1 | Planner history records the actual tool outcome (`success == actual result`). | I | HELD (`soul-agent-core`: `record_tool_outcome` passes real `tool_ok`, not a hardcoded literal, to `planner.history.record`; `planner_history_records_actual_outcome_not_hardcoded_success`, `planner_history_all_failures_yields_zero_success_rate`) |
| INV-PLAN-2 | Autonomous execution has hard budgets (turns, tool calls, writes, time, tokens). | I-2 | HELD for turns/tool-calls/writes/wall-clock (`soul-agent-core`: `AgentConfig::{max_turns,max_tool_calls,max_write_operations,max_wall_clock_secs}` enforced in `run_task`'s turn loop and tool-dispatch loop — over-budget tool calls are refused per-call with a `BLOCKED:` result, and the wall-clock budget aborts the run; `agent_config_default_has_sane_budgets`, `new_agent_starts_with_zeroed_budgets_and_no_task_start`). Token budget not yet wired — follow-up. |
| INV-PLAN-3 | Emergency stop denies new side effects and is durable across restart. | I-2 | HELD (`soul-agent-core::emergency_stop::EmergencyStop` — a file-backed latch checked at `run_task` entry and on every turn/tool-dispatch iteration; durability across restart is by construction, since the check is a filesystem read with no in-memory state to lose, not an `AtomicBool`; `trip_does_not_survive_only_in_memory_a_fresh_handle_sees_it`, `run_task_refuses_to_start_when_emergency_stop_already_tripped`, `run_task_starts_normally_when_emergency_stop_untripped`, `trip_emergency_stop_trips_latch_and_aborts_running_flag`) |

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
