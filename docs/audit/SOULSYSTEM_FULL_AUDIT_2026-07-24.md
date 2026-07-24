# SoulSystem full framework audit — 2026-07-24

## Executive result

SoulSystem is buildable and usable as a local-first agent from a fresh clone.
The canonical CLI exposes setup, health diagnostics, interactive, one-shot,
planning, gateway, and service modes. The fast install path is now a single
verified shell command, with npm and Cargo alternatives.

The audited core is green: formatting, strict Clippy, supply-chain policy,
RustSec audit, CLI smoke tests, and more than 1,900 focused unit, integration,
property, adversarial, persistence, and documentation tests pass. A CCOS
100,000-cycle performance test also passes in release mode.

Remote or multi-user production deployment is not yet declared ready. Secret
entry/storage, authentication for `soulsystem-lite`, TLS termination, and the
intended deployment/data model require owner decisions.

## Scope and provenance

The local branch was first fast-forwarded from
`Memorithm/SoulSystem@b3974aeb55687fb694067b4adada865ba24e30b8`. Component
imports are pinned in [UPSTREAM_COMPONENTS.md](../UPSTREAM_COMPONENTS.md):

- SciRust: `Memorithm/scirust@25f272a0506c9e67dd15051f1ac3235bfdd13e3d`
- CCOS: `Memorithm/CCOS@aaa941df0e54c5f8d4bf9b11a1797565d55331dc`
- CERVO: `Memorithm/cervo@bd1ef1687158774f57454d3a687bd6379819e4b0`

The resulting root workspace contains 184 packages. GPU-specific crates remain
in their dedicated workspace so the default CPU build does not require CUDA.

## Installation and ergonomics

### Ready

- One-line Linux/macOS installer:
  `curl -fsSL https://raw.githubusercontent.com/Memorithm/SoulSystem/main/install.sh | sh`
- Release archives are verified against their published SHA-256 before
  extraction; failure is fatal.
- Unsupported/missing release artifacts fall back to a source build.
- npm installer uses the same repository and checksum policy.
- `soulsystem --setup` provides first-run configuration.
- `soulsystem --doctor` checks directories, bubblewrap, the selected LLM
  provider, and gateway exposure/authentication without starting the agent.
- `--doctor` understands Ollama, OpenAI-compatible, and Anthropic endpoints and
  never embeds provider secrets in its URL or output.
- The current local smoke test reports `soulsystem 0.6.0` and `Doctor result:
  ready`.
- English and French guides now point to `Memorithm/SoulSystem`; stale fork
  links and two invalid `cgit` commands were removed.

This reaches the main Hermes Agent usability benchmark—one command to install,
then guided setup and a diagnostic command. Hermes still has broader bootstrap
coverage: its official installer supports native Windows and installs more
third-party prerequisites automatically. SoulSystem currently targets
Linux/macOS and relies on a source fallback when no release binary exists.

### Remaining ergonomic gaps

- Linux arm64 is recognized by the installer but absent from the release build
  matrix. Jetson/aarch64 users therefore compile from source.
- There is no native PowerShell installer.
- The 184-package monorepo makes a source fallback substantially slower than a
  prebuilt installation.
- The nested OctaSoma manifest defines profiles that Cargo ignores under the
  root workspace; move those profiles to the root to remove the warning.

## Component links and runtime integration

- CCOS, SciRust, and CERVO are real vendored directories, not host-specific
  symlinks or nested Git repositories.
- Local path dependencies are complete enough for `cargo metadata` and the
  audited package graph to resolve offline after dependency download.
- CCOS memory caches use synchronized `RwLock` state so the autonomous runtime
  remains `Send`/`Sync` under Tokio.
- CCOS snapshots retain atomic, fsync-backed durable writes.
- SoulBridge resolves CCOS and OctaSoma through an explicit environment
  override, `PATH`, sibling binaries, then repository-relative release paths.
- Operator scripts derive their root from the script location and contain no
  hard-coded `/root` runtime path.
- CERVO is linked into SoulBridge and the main runtime; its MCP server and
  `soulsystem-lite` are workspace members.

## Security assessment

### Controls verified

- Tool names are registered and unknown tools fail closed.
- Capability classification is centralized; unknown capabilities receive the
  most restrictive class.
- Filesystem operations enforce configured roots and reject traversal,
  `.git` mutation, and symlink escapes.
- Shell execution routes through the sandbox policy; bubblewrap is detected by
  `--doctor`.
- Agent output is screened before memory persistence.
- Gateway operator, disclosure, and state-changing routes require constant-time
  bearer-token validation. Health remains public; webhook routes use their own
  fail-closed signatures/secrets.
- Production posture evaluates the exact `GatewayAuth` instance passed to the
  serving path. It does not claim authentication for the local API or WebSocket
  bridge.
- Secret-pattern scan found only test fixtures/documentation examples.
- Installers verify release integrity.
- `quinn-proto` was upgraded from vulnerable 0.11.14 to 0.11.15, resolving
  RUSTSEC-2026-0185.
- `cargo audit` reports zero blocking vulnerabilities.
- `cargo deny check` passes advisories, bans, licenses, and sources.

### Residual security debt

- The setup wizard echoes provider secrets and may persist them in plaintext.
  Use environment variables or an external secret manager until hidden input
  and a cross-platform keystore contract are implemented.
- `soulsystem-lite` is loopback-only by default but its mutation endpoints do
  not authenticate. It must remain local-only until middleware is added.
- TLS is not provided by the runtime. Any remote gateway needs an authenticated
  reverse proxy or a future native TLS path.
- RustSec reports 18 allowed warnings, including unmaintained transitive crates
  and `lru` 0.12.5 unsoundness in an Avid/Ratatui chain. They are recorded in
  `deny.toml`; they should be removed as upstream replacements become
  available.
- A final threat model is pending confirmation of exposure, tenancy, and data
  sensitivity. Until then the safe operating assumption is one trusted user,
  loopback listeners, local workspace data, and no public ingress.

## Functional and quality validation

Successful checks:

```text
cargo metadata --no-deps
cargo fmt --all -- --check
cargo clippy <audited packages> --all-targets -- -D warnings
cargo deny check
cargo audit
cargo test <audited packages> -- --skip stress_100k_cycles_stays_bounded
cargo test --release -p ccos --test benchmark stress_100k_cycles_stays_bounded
cargo run --bin soulsystem -- --version
cargo run --bin soulsystem -- --help
cargo run --bin soulsystem -- --doctor
bash -n scripts/*.sh
node --check packaging/npm/scripts/install.js
git diff --check
```

Coverage includes CCOS causal memory, tamper-evident logs, deterministic replay,
prompt-injection guards, cold-tier persistence, crash recovery, graph
invariants, CERVO evolution/dynamics, SciRust numerics and autodiff,
SoulBridge, sandboxed tools, gateway authentication, and CLI diagnostics.

The CCOS benchmark is intentionally a release-profile test: it failed its
throughput assertion in debug mode but passed 100,000 cycles in 17.72 seconds
with optimizations, while all debug-mode correctness invariants passed.

## Prioritized follow-up

1. Decide the deployment contract and data sensitivity, then finalize the
   repository threat model.
2. Replace echoed/plaintext setup secrets with hidden input plus OS keychain or
   explicit environment-only storage.
3. Add authentication to `soulsystem-lite` before supporting non-loopback use.
4. Add Linux arm64 and Windows release/install paths.
5. Reduce the tracked RustSec debt and converge duplicate major dependency
   versions, particularly Axum and Reqwest.
6. Move nested profile settings to the root workspace and keep the strict
   Clippy/supply-chain gates required in CI.

Detailed finding status is tracked in
`.planning/phases/99-soulsystem-audit/99-UAT.md`.
