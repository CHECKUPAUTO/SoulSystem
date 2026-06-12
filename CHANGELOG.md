# Changelog

All notable changes to SoulSystem.

## [13.8.0] — 2026-06-09

### Audit Fixes (20 findings, 15 completed)

- **soul-agent-core**: Compaction failure now logs warning and falls back to truncation (was silent)
- **soul-agent-core**: Self-distillation errors now logged with tracing::warn (was silent if-let)
- **soul-conversations**: Mutex poisoning handled via `LockPoisoned` error (was unwrap panic)
- **soul_repl**: Runtime creation failures are fatal-safe with exit(1) (was panic)
- **soul_llm**: Ollama URL configurable via `OLLAMA_URL`, `OLLAMA_HOST`, `OLLAMA_MODEL` env vars
- **soul-critique**: Added `llm_critique_with_fallback()` — falls back to heuristic on LLM error
- **soul-dashboard**: Added `SharedCostSummary` (Arc<RwLock>) for thread-safe cost tracking
- **soul-protocol**: Added `spawn_discovery_server()` (non-blocking) alongside blocking version
- **soul-skills**: Parser now validates structure (warns on missing header/triggers/description)
- **soul-compaction**: Added 4 real-world tests (conversation, massive tool output, system prompt preservation)
- **soul-graph-memory**: Pathfinding upgraded from BFS to Dijkstra (respects edge weights)
- **soul-inference**: CostTracker now has `save()`/`load()` persistence to JSON
- **soul-wasm**: Fixed fd_write host function, cleaned up unused variables
- **soul-browser**: Regex compiled lazily via `lazy_static!` (was recompiled per call)
- **soul-webfetch**: `WebFetcher::new()` returns `Result` instead of panicking on builder failure
- **soul-sandbox**: Added kernel version check before applying seccomp (graceful skip on old kernels)
- **All crates**: Fixed 6 compiler warnings (unused variables, mutable bindings)

### Previous Changes (v13.7.0)

- **soul-wasm v0.2.0**: Real wasmtime runtime execution (45.0.1)
- **soul-critique v0.2.0**: LLM-based evaluation via Ollama
- **soul-conversations → REPL**: Auto-persistence, graph context injection
- **soul-dashboard**: Cost tracker visualization
- **soul-sandbox v0.2.0**: Seccomp-bpf enforcement via prctl
- **soul-protocol v0.2.0**: Workflow orchestration engine
- **A2A auto-discovery**: UDP broadcast discovery
- **Trading engine tests**: 4 new tests
- **Rustdoc API docs**: Generated for 10 core crates

## [13.6.0] — 2026-06-09

### Added — Improvements
- **soul-wasm v0.2.0**: Real wasmtime runtime execution (was validation-only, now compiles and runs WASM with memory limiting, WASI host functions, entry point detection)
- **soul-critique v0.2.0**: LLM-based evaluation via Ollama (`llm_critique()` function — parses JSON scores from LLM response, 6 quality dimensions)
- **soul-conversations → REPL**: Auto-persistence of `ask` conversations to SQLite (`conversations new/list/active/switch/stats` commands)
- **Graph memory → REPL**: `context_for_query()` injects relevant graph nodes into agent prompts (keyword matching, relationship expansion)
- **soul-dashboard v13.5.0**: Cost tracker visualization (`/api/costs` endpoint — CostEntry, CostSummary, per-model breakdown)
- **soul-sandbox v0.2.0**: Real seccomp-bpf enforcement via prctl (NO_NEW_PRIVS + SECCOMP_MODE_FILTER, x86_64 syscall allowlist)
- **soul-protocol v0.2.0**: Workflow orchestration engine (WorkflowStep with dependencies, DAG execution, cycle detection)
- **A2A auto-discovery**: UDP broadcast discovery server/client (AgentDirectory.start_discovery_server/discover_network)
- **Trading engine tests**: 4 new tests (ShadowConfig, PortfolioSnapshot, ShadowEvaluator)

### Tests
- 94 total tests passing (was 82)

## [13.6.0] — 2026-06-09

### Added — Infrastructure & Integration
- **soul-mcp v0.1.0**: MCP client/server — JSON-RPC 2.0, `FnMcpHandler`, `WsTransport` (WebSocket), `ToolRegistry` (10 tests)
- **soul-protocol v0.1.0**: Agent Protocol (AutoGPT-compatible), `AgentMesh`, `AgentDirectory`, `A2AServer`, `A2AClient` (13 tests)
- **soul-skills v0.1.0**: `.skills/` format — load/save/match, priority, triggers, built-in skills, Markdown parser (7 tests)
- **soul-graph-memory v0.1.0**: Knowledge graph — typed nodes/edges, BFS pathfinding, cycle detection, topological sort, persist/load (11 tests)
- **soul-compaction v0.1.0**: 4-pass context compression — Reclaim→Shrink→Collapse→Evict (9 tests)
- **soul-inference v0.1.0**: 3-axis inference control — capability×thinking×context, `CostTracker` (14 tests)
- **soul-critique v0.1.0**: Self-critique loop — 6 quality dimensions, Reflexion loop, `quick_critique()` (11 tests)
- **soul-designtree v0.1.0**: OpenSpec lifecycle — Idea→Research→Decision→Spec→Implementation→Testing→Verified/Abandoned (9 tests)
- **soul-wasm v0.1.0**: WASM plugin sandbox — `PluginRegistry`, `WasmPlugin`, validation, `SandboxConfig` (11 tests)

### Added — Wiring & Integration
- Compaction auto-activates at 80% context threshold before LLM calls
- Self-critique evaluates every `run_task()` output; failures trigger SafetyWarning
- Inference controller auto-selects model by task complexity in REPL `ask`
- `save`/`load` REPL commands persist graph + design tree + agent context
- MCP tools/call/server/connect/ws commands in REPL
- Design tree auto-creates nodes on `run` (Implementation→Testing lifecycle)
- `AgentMesh` registers/unregisters agents, routes tasks, broadcasts events
- `A2AServer` serves Agent Protocol over WebSocket; `A2AClient` connects to remote agents

## [13.5.0] — 2026-06-08

### Added — Autonomous Entity
- **soul-agent-core**: Autonomous agent core — ReAct loop, safety, task queue, self-evolution (NEW CRATE)
- **soul_llm v0.2.0**: ChatSession, conversation context, streaming, tool schemas, 7 built-in tools
- **soul_planner v0.2.0**: LLM-powered planning — create_plan_llm(), decide_llm(), memory distillation
- **soul_tools v0.2.0**: Async shell executor, permission model (Read/Write/Destructive), file ops (read/write/patch/search/grep)
- **soul_repl v0.2.0**: Conversation REPL with autonomous task execution, streaming events, plan mode
- 30 tests added (19 soul_tools + 11 soul_planner)

### Added — Previous
- **soullink-shm**: Zero-copy IPC via memfd + mmap + UDS fd-passing (8 tests)
- **soullink-vram**: Dynamic VRAM management with 5 priority levels (4 tests)
- **soullink-registry**: Distributed service registry with serialize/merge (6 tests)
- **soullink-trainer**: Fine-tuning pipeline with trajectory recording + DPO pairs (5 tests)
- **soullink-memory-hierarchy**: Working/episodic/semantic memory with consolidation engine (4 tests)
- **soullink-moe**: Mixture of Experts — task classifier + expert router (8 tests)
- **soul-top**: Real-time TUI visualizer built with Ratatui (3 tests)
- **soul-chaos**: Chaos Monkey for resilience testing — 5 fault types (8 tests)
- **soul-shell**: Interactive CLI for kernel communication (5 tests)
- **soulsystem-common**: Shared types — embedder, config, memory types, health, metacognition (30 tests)
- **soullink-circuit**: Unified circuit breaker (3 implementations → 1) (8 tests)
- Trading core types: `MarketState`, `Bar`, `Order`, `Trade`, `Symbol`, `Exchange`, `Polarity`, `Reliability`

### Changed
- Bus unification: merged `bus/` + `soullink-bus/` into unified `Message` enum
- Migrated 35 crates to workspace dependencies (~80 conversions)
- Upgraded workspace: axum 0.7→0.8, tower 0.4→0.5, tower-http 0.5→0.6, metrics 0.22→0.23
- openevolve now uses workspace deps (axum, tower, metrics)
- Memory hub uses `SciRustEmbedder` from soulsystem-common

### Fixed
- scirust-learning malformed serde dependency (`0.6.0]` → workspace)
- toml workspace dependency missing serde feature
- openevolve `toml::to_string_pretty` → `toml::to_string` (API removed in toml 0.8)
- local-skills missing `tempfile` dev-dependency
- forge-bridge example missing `Candidate` trait import
- scirust-trading-core missing `EventBus` re-export
- soullink-node thiserror 1.x → 2.x

### Removed
- 3 dead code items: `EdgeMeta` struct, `_ensure_debug()`, `_ensure_itemkind_used()`
- 3 unnecessary `#[allow(dead_code)]` annotations
- 3 broken trading examples (reference future API not yet implemented)
- 39 orphaned .rs files from soullink-brain root

## [0.6.0] — 2026-05-11

### Added
- Initial SoulSystem unified monorepo
- OpenClaw-U kernel, SoulLink HNN Mesh, Clawd Assistant, AVID Engineering
- 87 workspace crates, ~1194 Rust files, ~254K LOC
