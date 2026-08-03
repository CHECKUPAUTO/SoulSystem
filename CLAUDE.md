# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Test Commands

```bash
# Full workspace build (release)
cargo build --release

# Fast check (no codegen)
cargo check

# Run all workspace tests
cargo test --workspace

# Test a specific crate
cargo test -p soul-memory
cargo test -p soullink-core

# Run only lib tests (skip integration)
cargo test --lib -p <crate>

# Clippy on workspace
cargo clippy --workspace

# Clippy on a single crate
cargo clippy -p soullink-circuit

# Format
cargo fmt

# Validate script (check + test + clippy + stub check)
./scripts/validate.sh
./scripts/validate.sh --fast          # cargo check only
./scripts/validate.sh --project <name> # single project

# Makefile targets
make validate        # full validation pipeline
make validate-fast   # cargo check only
make test            # unit tests
make ci              # full CI pipeline

# Run the main binary
cargo run -- [--dev] [--repl] [--daemon]

# Run the autonomous REPL
cargo run -p soul_repl --release

# Dependency audit
cargo deny check
```

## Architecture Overview

SoulSystem is a Rust monorepo (~100+ workspace crates) forming an autonomous digital agent ecosystem. The workspace resolver is v2, edition 2021, minimum Rust 1.75.

### Major Subsystems

**1. SoulLink Neural Mesh** (`soullink-brain/`) — 40+ crates forming a Hamiltonian Neural Network (HNN) with 6 organs (Science, Mind, Engineer, Crypto, Creative, Meta). Key crates:
- `soullink-core` — HNN engine, Verlet symplectic dynamics
- `soullink-orchestrator` — organ coordination
- `soullink-memory` / `soullink-memory-hierarchy` — memory systems
- `soullink-circuit` — circuit breaker / rate limiting
- `soullink-gate` / `soullink-gateway` — access control
- `soullink-reasoning` / `soullink-inference` — reasoning engine
- `soullink-moe` — mixture of experts
- `soullink-rag` — retrieval augmented generation
- `soullink-bus` — internal message bus
- `soullink-autonomy` — autonomous decision making
- `soullink-senate` — multi-agent voting/consensus

**2. Autonomous Entity** (`soul_llm`, `soul_planner`, `soul_tools`, `soul_repl`, `soul-agent-core/`) — ReAct loop agent (observe→think→act→evaluate):
- `soul_llm` — ChatSession, streaming, native Ollama tool calling
- `soul_planner` — LLM-powered goal decomposition
- `soul_tools` — async shell, file ops, permission-gated tools
- `soul_repl` — conversational REPL with real-time streaming
- `soul-agent-core` — core agent loop

**3. Infrastructure Crates** (`soul-*`):
- `soul-memory` — vector storage (sled-backed)
- `soul-daemon` — background goal processing
- `soul-sandbox` — sandboxed execution
- `soul-bridge` — unified bridge (replaces 9 individual bridge crates)
- `soul-eventbus` — event bus
- `soul-scheduler` — task scheduling
- `soul-mcp` — MCP protocol support
- `soul-skills` — skill system
- `soul-goaltree` — goal tree management
- `soul-automodify` — self-modification
- `soul-browser` — browser automation
- `soul-subagents` — subagent management
- `soul-webfetch` — web fetching
- `soul-compaction` — memory compaction
- `soul-protocol` — protocol definitions
- `soul-designtree` — design tree
- `soul-inference` — inference engine
- `soul-critique` — critique/reflection

**4. AVID Ecosystem** (`avid/crates/`) — 24 crates for web exploration and API cloning:
- `avid-core`, `avid-cortex`, `avid-scout` (753 web extraction modules)
- `avid-tokenjuice` (96 compaction rules for CLI tools)
- `avid-model-router` (task classification → local/remote dispatch)
- `avid-orchestrator`, `avid-sandbox`, `avid-critic`, `avid-anticlone`

**5. SciRust Framework** (`scirust-*`) — scientific computing:
- `scirust-core`, `scirust-simd`, `scirust-gpu`, `scirust-autodiff`
- `scirust-trading-*` — trading engine pipeline (core, engine, observer, persistence, news, monitor)

**6. Root Binary** (`src/`) — `soulsystem` binary with modules:
- `config`, `telemetry`, `audit_log`, `code_signing`, `bus`, `compute_backend`
- `memory_hub`, `rag_middleware`, `self_healer`, `circuit_breaker`
- `ws_bridge`, `autonomous`, `metrics`, `backup`, `discovery`

**7. Shared Crates** (`crates/`):
- `soulsystem-common` — shared types
- `soul-top` — system monitoring
- `soul-chaos` — chaos testing
- `soul-shell` — shell utilities
- `soul-dashboard` — web dashboard

### Key Patterns

- **Workspace dependencies** are centralized in the root `Cargo.toml` `[workspace.dependencies]` section
- **Feature flags**: `dev` (dashboard + anomaly detection), `gpu` (CUDA support)
- **Release profile**: LTO fat, 1 codegen unit, stripped
- **Bridge unification**: 9 individual bridge crates were consolidated into `soul-bridge` with module aliases (`avid_bridge`, `brain_bridge`, `mesh_bridge`, etc.)
- **Security**: BoundSystem sandbox (bubblewrap + seccomp), code signing, circuit breaker
- **Validation**: `scripts/validate.sh` runs check → test → clippy → stub detection per project
- **Cargo config** silences `unexpected_cfgs`, `dead_code`, `unused_imports` warnings globally
- **Cargo deny** is configured for dependency auditing (`cargo deny check`)
