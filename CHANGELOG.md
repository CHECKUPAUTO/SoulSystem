# Changelog

All notable changes to the AVID project are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] — 2026-05-06

### Added

- **avid-anticlone** crate — Python AST fingerprinting via `rustpython-parser`
  - Node histogram (55 AST node types: 28 statements, 27 expressions)
  - Call-graph edge extraction with function context tracking
  - AST depth profile collection (max, mean, stddev)
  - Weighted Jaccard similarity: 0.4 × nodes + 0.4 × edges + 0.2 × depth
  - Configurable originality threshold with `check()` API
  - Corpus-based clone detection tests

- **avid-sandbox** crate — Hardened Python subprocess runner
  - CPU, memory, process, file, and FD rlimits
  - `PR_SET_NO_NEW_PRIVS` — no privilege escalation
  - Process group isolation with `setpgid` + `killpg` on timeout
  - Optional network namespace isolation via `unshare -n`
  - Configurable wall timeout with SIGKILL
  - Captured stdout/stderr with size caps and truncation markers
  - Tests: hello world, OOM kill, network isolation, infinite loop timeout

- **avid-core** crate — Application logic
  - Planner agent — task decomposition into structured plans
  - CoreDesign agent — Python code generation with AST validation
  - Critic agent — quality scoring and originality verification
  - LLM client — reqwest-based Ollama API with JSON mode and exponential backoff
  - Orchestrator — background worker loop with agent pipeline coordination
  - Redis-backed task queue with `AsyncCommands` trait
  - SQLite fallback queue (WAL mode, r2d2 pool)
  - Memory store — SQLite-based result/trace persistence
  - Garde-validated input models
  - Prometheus metrics: `avid_tasks_total`, `avid_agent_latency_ms`, `avid_anticlone_score`, `avid_queue_depth`
  - Tracing JSON logging with configurable levels

- **avid-server** crate — HTTP API
  - `GET /healthz` — component health with degraded/ok status
  - `POST /tasks` — task submission with validation
  - `GET /tasks/{task_id}` — result retrieval with plan, execution, and originality
  - `GET /metrics` — Prometheus text format
  - Constant-time API token authentication via `subtle`
  - Tower-http middleware: tracing, 64KB body limit

- **Infrastructure**
  - `install.sh` — system dependency installer (Rust 1.78+, Python 3)
  - `run.sh` — server launcher with env validation
  - `justfile` + `Makefile` — command runners
  - `.github/workflows/ci.yml` — CI pipeline
  - Quality gate: `#![forbid(unsafe_code)]`, `#![deny(warnings)]`, pedantic+nursery clippy
  - LLD linker configuration for `x86_64-unknown-linux-musl`

[0.1.0]: https://github.com/CHECKUPAUTO/AVID/releases/tag/v0.1.0
