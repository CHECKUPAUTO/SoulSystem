# SoulSystem Production Hardening Plan

Baseline: `9d2f82783d87c3dad50eade02ce2c96d90c628f5` (rustc 1.94.1, cargo 1.94.1).

This plan turns SoulSystem from a green-compiling pre-production monorepo into a
system that can be operated safely in production. It is delivered as a
dependency-ordered series of **small, individually reviewable** pull requests.
No PR is auto-merged; each requires explicit repository-owner approval, and a
dependent PR does not start until its prerequisite is merged (or is explicitly
stacked and documented).

The older static audit (`docs/audit/SOULSYSTEM_FULL_AUDIT_2026-07-21.md`,
baseline `5e3b0c3b`) is a **lead**, not current truth. Every finding is
re-verified against current `main` in [`findings.json`](findings.json) and
classified `CONFIRMED_CURRENT` / `PARTIALLY_FIXED` / `FIXED_BY_PRIOR_CHANGE` /
`NOT_REACHABLE` / `FALSE_POSITIVE` / `REQUIRES_RUNTIME_VERIFICATION`.

## Engineering rules (non-negotiable)

- No stub, placeholder, `TODO`, `FIXME`, `unimplemented!()`, or `todo!()`.
- Never delete/ignore tests, add `|| true`, use `continue-on-error` for required
  checks, weaken `-D warnings`, or globally suppress lints to hide a defect.
- Never silently flip a policy from fail-closed to fail-open, or broaden
  filesystem/network permissions.
- Claims of sandboxing/TLS/auth/signing/zeroization require a proven active
  runtime path and a test.
- No unrelated refactoring inside a security PR; no giant one-shot PR.

## PR sequence

| PR | Title | Establishes | Depends on |
|----|-------|-------------|------------|
| **A** | Re-baseline + production fail-closed guard | INV-ENV-1/2/3; findings register; Phase-0 docs | — |
| **B** | Reject unknown tools + typed registry | INV-TOOL-1 | A |
| **C** | Typed capabilities + central policy engine | INV-TOOL-2 | B |
| **D** | Mandatory OS-isolated process executor | INV-EXEC-1/2/3/4 | C |
| **E** | Canonical filesystem roots + atomic writes | INV-FS-1/2/3/4 | C |
| **F** | Authentication + webhook verification | INV-NET-1/3 | A |
| **G** | TLS + request limits + CORS + rate limiting | INV-NET-2/4/5 | F |
| **H** | Screen-before-persist + memory trust | INV-MEM-1/2/3/4 | C |
| **I** | Planner correctness + budgets + emergency stop | INV-PLAN-1/2/3 | — |
| **J** | Secret zeroization + key lifecycle | INV-SEC-1/2 | — |
| **K** | Signed + gated self-modification | INV-MOD-1/2 | C, J |
| **L** | Atomic persistence + recovery | INV-PERSIST-1/2 | — |
| **M** | Provider resilience + observability | INV-TRUTH-3 | — |
| **N** | Runtime + provider consolidation | canonical runtime | B–M |
| **O** | Placeholder elimination + experimental gates | INV-TRUTH-1/2 | N |
| **P** | CI, fuzz, release, container, deploy hardening | INV-CI-1/2/3 | all |

## Per-PR workflow

1. Fetch latest `origin/main`; verify a clean tree.
2. Branch from the exact current `origin/main`.
3. Reproduce the defect with a failing test or a documented negative command.
4. Implement the smallest complete correction.
5. Add unit, integration, negative, and regression tests.
6. Run focused validation, then the full repository gate (see below).
7. Open a **draft** PR with the mandated evidence sections.
8. Stop for owner approval before starting the dependent PR.

## Full validation gate (every Rust PR)

```bash
cargo fmt --all -- --check
cargo metadata --locked --no-deps --format-version 1 > /tmp/root-metadata.json
cargo metadata --manifest-path scirust-gpu/Cargo.toml --no-deps --format-version 1 > /tmp/gpu-metadata.json
cargo check  --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test   --locked --workspace --lib --exclude soulsystem --exclude soul-kernel
cargo test   --locked -p soulsystem --tests -- --test-threads=2
cargo check --manifest-path scirust-gpu/Cargo.toml --all-targets
cargo clippy --manifest-path scirust-gpu/Cargo.toml --all-targets -- -D warnings
```

Plus, once introduced as gates: `cargo deny check`, `cargo audit`, the feature
matrix, and the new security integration tests.

## Status

PR A is the first delivery. Its scope is deliberately narrow: the re-verified
findings register, the four Phase-0 documents, and the fail-closed startup guard
(`soul-prod-guard` crate + wiring in the `soulsystem` binary). It changes no tool
dispatch, no gateway, and no persistence behaviour — those are PRs B onward.
