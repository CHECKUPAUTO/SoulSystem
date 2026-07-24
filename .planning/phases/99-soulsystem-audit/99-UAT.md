# SoulSystem audit UAT — 2026-07-24

Baseline: `Memorithm/SoulSystem@b3974aeb55687fb694067b4adada865ba24e30b8`

This UAT records findings after synchronizing the GitHub baseline and importing
the current CCOS, SciRust, and CERVO components. Completed findings remain here
for traceability.

## Findings

### F-01 — High — Component refresh broke Tokio thread safety

CCOS used `RefCell` for caches contained in shared causal-memory state. This
made `AutonomousAgent` futures non-`Send` and prevented the canonical runtime
from compiling. Files: `ccos/src/memory.rs`, `ccos/src/external_memory.rs`,
`ccos/src/cold_index.rs`.

Status: fixed with synchronized caches; targeted `cargo check` passes.

### F-02 — High — CCOS refresh regressed durable persistence

The refreshed CCOS snapshot and split-state writers used bare
`std::fs::write`, reintroducing crash-corruption risk already fixed in
SoulSystem. Files: `ccos/src/persist.rs`, `ccos/src/persistence.rs`.

Status: fixed by routing both paths through `util::write_durable`.

### F-03 — High — Installers referenced the previous repository and did not verify releases

The shell and npm installers downloaded release archives from
`CHECKUPAUTO/SoulSystem` without checking the published SHA-256 file. Files:
`install.sh`, `packaging/npm/scripts/install.js`,
`packaging/npm/package.json`, `README.md`.

Status: fixed; syntax checks pass.

### F-04 — High — Operator scripts and bridge fallbacks contain machine-specific `/root` paths

Fresh installations cannot locate SoulSystem, CCOS, or OctaSoma because
operator scripts and bridge defaults assume the original development machine.
Files: `scripts/start-soulsystem.sh`, `scripts/status.sh`,
`scripts/tmux-session.sh`, `scripts/ecosystem-test.sh`,
`soul-bridge/src/ccos.rs`, `soul-bridge/src/octasoma.rs`.

Status: fixed in `22bf709d`; paths resolve through explicit environment
overrides, `PATH`, sibling binaries, and repository-relative fallbacks. The
bridge's 28 tests and all script syntax checks pass.

### F-05 — Medium — `--doctor` probes an Ollama-only path for every provider

The new diagnostic command checks `/api/tags` even when the selected provider
is OpenAI or Anthropic, producing a false warning. File: `src/main.rs`.

Status: fixed in `9069a0f2`; Ollama uses `/api/tags`, OpenAI-compatible and
Anthropic providers use `/v1/models`, and credentials stay in provider-specific
headers. Two focused tests pass.

### F-06 — Medium — Production posture still reports the authenticated gateway as unauthenticated

`src/prod_guard.rs` hardcodes every listener's `authenticated` flag to false,
although `soul_gateway` now enforces the configured bearer token on operator
routes. This makes readiness evidence stale. Files: `src/prod_guard.rs`,
`soul_gateway/src/lib.rs`.

Status: fixed in `09202943`; the guard evaluates the exact `GatewayAuth`
instance consumed by the server. The local API and WebSocket bridge remain
explicitly unauthenticated and fail closed in production posture.

### F-07 — High — Setup secret entry is echoed and persisted as plaintext

`prompt_password` reads from an echoed terminal and the setup config can store
provider keys directly. File: `src/main.rs`.

Status: manual-only; requires a cross-platform secret-storage decision.

### F-08 — Medium — `soulsystem-lite` exposes mutation endpoints without authentication

The lightweight REST server binds to loopback, which limits exposure, but its
CERVO/CCOS mutation routes have no bearer middleware. File:
`soulsystem-lite/src/main.rs`.

Status: manual-only; decide whether lite is strictly local-only or a supported
remote deployment before choosing its authentication contract.

### F-09 — Medium — Claimed Linux arm64 prebuilt install has no release job

The installer recognizes `aarch64-unknown-linux-gnu`, but the release matrix
does not produce that artifact, forcing a source build on Jetson/Linux arm64.
Files: `install.sh`, `.github/workflows/release.yml`.

Status: manual-only; requires a native or cross-compilation runner/toolchain
decision.

### F-10 — Medium — Threat priority depends on unresolved deployment context

Internet exposure, tenancy, and data sensitivity were not defined. These
assumptions materially affect gateway, MCP, sandbox, and secret-handling risk.

Status: manual-only pending owner clarification.

### F-11 — High — `quinn-proto` allowed remote memory exhaustion

The refreshed lockfile selected `quinn-proto` 0.11.14, affected by
RUSTSEC-2026-0185 (unbounded out-of-order stream reassembly).

Status: fixed by updating the lockfile to 0.11.15. `cargo audit` and
`cargo deny check` now pass with no blocking vulnerability.

### F-12 — Medium — Strict lint gate failed across imported components

Current Rust/Clippy rejected stale lint names and several mechanical patterns
in SciRust, CCOS, CERVO, SoulBridge, and SoulLink dependencies.

Status: fixed; the audited package set passes `cargo clippy --all-targets --
-D warnings`.
