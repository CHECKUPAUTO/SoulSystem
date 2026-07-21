# SoulSystem Complete Read-Only Audit Report

**Date:** 2026-07-21
**Auditor:** OpenCode AI
**Mode:** Strictly read-only — no files were modified, created, or executed during the audit.

---

## 1. Executive Summary

SoulSystem is an extraordinarily ambitious Rust monorepo (~100+ workspace crates) spanning autonomous agents, neural mesh computing, scientific computing, trading, web automation, and industrial operations. The repository contains **56+ executable binaries**, **~45,000 lines of production-quality deep learning framework code** (scirust-core), **multiple competing agent runtimes**, and **extensive but fragmented security primitives**.

**Core finding: The repository is a pre-production monorepo in active reorganization.** While individual crates contain impressive implementations (scirust-core, CCOS, soul_journal, soullink-circuit, soullink-gate, soul_persistence), the system as a whole has:

- **3 competing agent runtimes** (`soul-agent-core/AutonomousAgent`, `soul_entity/SoulEntity`, `soul-kernel`) with incompatible abstractions
- **Simulated autonomy** in the primary `SoulEntity` runtime (plans are hardcoded, execution is faked, evaluation is hardcoded 0.9)
- **No authentication on any network endpoint** — all gateway, API, WebSocket, and webhook routes are unauthenticated
- **No prompt injection filtering before memory persistence** — injection detection exists (soullink-gate/InjectionScanner) but is only applied to tool outputs returning to the LLM, not to content before it enters memory
- **Unreachable security systems** — code_signing, soullink-secrets, soullink-allowlist, semantic_firewall, soul_security (IntrusionDetector), soul_guard are never wired into any runtime path
- **Tool dispatch bypasses sandboxing** — `soul_tools::dispatch_tool()` and `execute_shell()` use bare `std::process::Command` directly, bypassing the sandbox used by `AsyncShellExecutor`
- **Massive fragmentation**: 13+ memory crates with overlapping responsibilities, 4+ graph implementations, 2+ sandbox systems, 2 concurrent Telegram bot integrations, 3 webhook implementations, abandoned duplicate crates

**The repository requires a multi-phase consolidation effort before it can be considered a production-ready competitor to OpenClaw or Hermes-Agent.**

---

## 2. Scope and Limitations

**Scope:** All Rust source files, Cargo manifests, CI configuration, scripts, Docker files, systemd units, and documentation in `/root/SoulSystem/`. Every `.rs` file in every reachable workspace member was read.

**Limitations:**
- `soul-neural/` crate has no `src/lib.rs` — may exist elsewhere or be empty
- GPU sub-workspaces (`workspaces/gpu`) were excluded per the Cargo.toml CI memory constraint note
- External services referenced but not built from this repo (qmd-mcp, NATS, Weaviate, ChromaDB) were treated as external dependencies
- The `target/` directory contents (build artifacts) were not inspected
- Git history was not analyzed — only current file state
- Some deep sub-modules within `openevolve`, `scirust-learning`, and `soullink-node/self_modify/` were examined at the module level but individual functions may have been missed in very large files

---

## 3. Repository Inventory

### 3.1 Workspace Statistics

| Category | Count |
|----------|-------|
| Workspace members (Cargo.toml) | ~175 |
| Excluded crates | ~23 |
| Library crates | ~130 |
| Binary crates | 56+ |
| Feature flags | 11 (dev, gpu, ed25519, avid, brain_system, soul_neural, organs, mesh, services, openevolve, synergie) |
| Duplicate package names | 3 (souls, soul_evolution, ccos, semantic_firewall all have os-agents/ duplicates) |
| Git dependencies | Not found (all workspace deps use `path =` or crates.io) |
| Proc-macro crates | 3 (scirust-gpu-macros, scirust-simd-macros, scirust-macros) |
| Build scripts | Not found |
| Fuzz targets | 6 (4 in root fuzz/, 2 in avid fuzz/) |
| Examples | ~15 across various crates |
| Benchmarks | ~5 criterion benches |

### 3.2 Root Package

**`soulsystem` v0.6.0** (root Cargo.toml line 302)

| Binary | Entry | Maturity |
|--------|-------|----------|
| `soulsystem` | `src/main.rs:133` | Pre-production — monolithic, all features compiled together |

### 3.3 Workspace Version

All workspace crates use `version = "13.5.0"` from `[workspace.package]`. The root package uses `version = "0.6.0"` independently — version inconsistency.

### 3.4 Excluded but Present Crates

Crates excluded from workspace but present on disk: `octasoma/octacore`, `soul-neural/`, `soullink-node/`, `turboquant/`, `os-agents/`, `intel-integrations/ironclaw`, `openclaw-gateway/`, `neural-store/`, `jit-agentic-engine/`, `soul-project/`, `avid-anticlone-service/`, `avid-soullink/`, `avid-rstdp/`, `turbovec/`.

### 3.5 Binary Entry Point Summary (56+)

Key binaries include: `soulsystem`, `souls`, `aevolve`, `clawd`, `soul-kernel`, `soullink`, `orchestrator`, `soullink-gateway`, `soullink-scan`, `soullink-trader`, `avid`, `avid-anticlone-server`, `forge`, `ccos`, 5 `soullink-organs` binaries, `openclaw-gateway`, `synergie`, `brain-system-rs`, `turboquant`, `openevolve`, `chronos-agent`, `slha-audit`, `ironclaw`.

### 3.6 Duplicated/Abandoned Crates

| Crate | Duplicate | Status |
|-------|-----------|--------|
| `soul-graph-memory` | `soul-memory/src/graph.rs` | ABANDONED |
| `soul-conversations` | `soul-memory/src/conversations.rs` | ABANDONED |
| `soul-persist` | `soul-memory/src/persist.rs` | ABANDONED |
| `soul_evolution` (os-agents) | `soul_evolution/` | DUPLICATE |
| `souls` (os-agents) | `souls/` | DUPLICATE |
| `soul_sandbox` (os-agents) | `soul_sandbox/` | DUPLICATE |
| `semantic_firewall` (os-agents) | `semantic_firewall/` | DUPLICATE |
| `ccos/CCOS` | `ccos/` | DUPLICATE |
| `scirust-chronos-agent` | `soullink-node/scirust/chronos-agent/` | DUPLICATE |

---

## 4. Architecture Map

```
┌──────────────────────────────────────────────────────────┐
│                    BINARIES (56+)                        │
└──────────────────────┬───────────────────────────────────┘
                       │
┌──────────────────────┴───────────────────────────────────┐
│             AGENT RUNTIMES (3 competing)                 │
│  soul-agent-core (AutonomousAgent)  ─── real ReAct loop  │
│  soul_entity (SoulEntity)           ─── SIMULATED loop   │
│  soul-kernel (kernel)               ─── real autonomous  │
└──────────────────────┬───────────────────────────────────┘
                       │
┌──────────────────────┼───────────────────────────────────┐
│     GATEWAYS         │      MEMORY SYSTEMS (13 crates)   │
│  soul_gateway (HTTP) │  soul-memory (sled+Qdrant+SQLite)│
│  soul_api (types)    │  soul_persistence (redb)          │
│  soul-mcp (MCP/WS)   │  soul_journal (mmap WAL)          │
│  soul_ipc (ringbuf)  │  soul-compaction (4-pass)         │
│  ws_bridge (WS)      │  ccos (causal+hash-chain)         │
│  clawd (Telegram)    │  soullink-memory (HNSW+concepts)  │
│                      │  soullink-memory-hierarchy (3-lay)│
│                      │  +3 ABANDONED duplicates          │
└──────────────────────┼───────────────────────────────────┘
                       │
┌──────────────────────┴───────────────────────────────────┐
│               INFRASTRUCTURE & SECURITY                   │
│  soul_sandbox (string-level + seccomp)                    │
│  bound-system (bubblewrap + whitelist)                    │
│  soullink-circuit (circuit breaker)    ─── ACTIVE         │
│  soullink-gate (ApprovalGate+Injection) ─── ACTIVE        │
│  soullink-security (pattern scanner)  ─── PARTIAL USE     │
│  soullink-secrets (AES-GCM)           ─── UNUSED          │
│  soullink-allowlist (domain filter)   ─── UNUSED          │
│  semantic_firewall (cosine filter)    ─── UNUSED          │
│  soul_security (rate limiter)         ─── UNUSED          │
│  soul_guard (compromise latch)        ─── UNUSED          │
└──────────────────────┬───────────────────────────────────┘
                       │
┌──────────────────────┴───────────────────────────────────┐
│           SCIENTIFIC COMPUTING (12 scirust crates)       │
│  scirust-core (45K LOC — autograd, 54 NN modules, etc.)  │
│  scirust-simd (AVX2/SSE2/NEON/SVE runtime dispatch)      │
│  scirust-autodiff (dual numbers, tape-based AD)           │
│  scirust-symbolic (symbolic math, parser, diff, solver)   │
│  scirust-learning (regression, pattern discovery)         │
│  scirust-reasoning (optimizer, equation solving)          │
│  scirust-trading-core (types, market domain model)        │
│  scirust_affective_core (PAD space, homeostatic drives)   │
└──────────────────────────────────────────────────────────┘
```

---

## 5. Runtime Inventory

### 5.1 Runtime Matrix

| Runtime | Entry Point | Planner | Model Path | Tool Dispatcher | Authorization | Sandbox | Memory | Reachable | Maturity | Recommendation |
|---------|------------|---------|-----------|----------------|--------------|---------|--------|-----------|----------|---------------|
| **soul-agent-core** `AutonomousAgent` | `run_task()` lib.rs:555 | StrategySelector (keyword heuristics) | `guarded_llm_chat()` → OllamaClient (circuit-breaker wrapped) | `async_dispatch_tool()` from soul_tools | YES — ApprovalGate from soullink-gate | YES — AsyncShellExecutor uses soul_sandbox | 6 systems (working, hierarchical, KG, CCOS, semantic, planner) | Via soul-daemon, soul-kernel | **Most complete** — real ReAct, PlanThenExecute, ToT | **CANONICAL** |
| **soul_entity** `SoulEntity` | `run_cycle()` entity.rs:370 | **SIMULATED** — 4 hardcoded steps | Optional LLM summary only | Not used for execution | NONE | YES — via execute_shell() → soul_sandbox | LongTermMemory, HierarchicalMemory, event store | Via souls binary --entity mode | **SIMULATED** | **DEPRECATE** |
| **soul-kernel** `kernel` | `heartbeat_loop()` main.rs:212 | GoalPlanner (priority queue) | `LlmEngine::reflect()` → OllamaClient | `Action::execute()` — 13 action types | Action-level security validation | Sandbox (for code patches only) | Weaviate vector DB + state files | Direct binary | **Real autonomous loop** | **MERGE actions** |
| **soul-daemon** | `Daemon::run()` lib.rs:203 | LLM task decomposition | OllamaClient + AutonomousAgent.run_task() | Through AutonomousAgent | Inherited | Inherited | PersistentStore (sled) | Via soul-daemon lib | **Wrapper** | **MERGE** |
| **souls binary** | runner.rs:434 | SIMULATED (/plan is echo) | Via soul_repl or SoulEntity | Not dispatched | NONE | Created but unused | Via SoulEntity | Direct binary | **Launcher** | **PRESERVE** |
| **soul_repl** | run_repl() lib.rs:70 | NONE (TUI only) | LlmClient::generate() | NONE (tools registered not executed) | NONE | Created but unused | Sessions to JSON | Library used by souls | **TUI shell** | **PRESERVE** |

---

## 6. Canonical Execution Paths

### 6.1 Current Most-Complete Path (soul-agent-core `AutonomousAgent`)

```
User Input
  ↓
AutonomousAgent::run_task(task)  [lib.rs:555]
  ↓
StrategySelector::select_with_failures(task, failures)  [strategy.rs:418]
  ├─ ReAct (default)
  ├─ PlanThenExecute (5+ failures)
  └─ TreeOfThoughts (10+ failures or complexity >= 0.6)
  ↓
run_react(task)  [lib.rs:702]
  ├─ Safety warnings at configured turn thresholds
  ├─ Inject memory context (working, hierarchical, KG, CCOS, metacognition)
  ├─ compact_if_needed() — 4-pass context compaction
  ├─ guarded_llm_chat(messages, tool_schemas)  [lib.rs:377]
  │   └─ CircuitBreaker(ollama-provider).call(|| OllamaClient::chat())
  │       └─ POST http://127.0.0.1:11434/api/chat
  ├─ Parse tool calls from response
  │   └─ For each tool_call:
  │       ├─ PermissionLevel classification
  │       ├─ ApprovalGate::evaluate(name, scope, req)
  │       ├─ async_dispatch_tool(name, args)  ← CRITICAL: bare std::process::Command
  │       ├─ ccos_observe_tool(name, args, output, ok)
  │       ├─ screen_tool_output(name, output)  ← AFTER persistence
  │       └─ Add tool result to chat session (truncated to 3000 chars)
  ├─ If no tool calls: remember_observation(content)
  └─ Check completion keywords
  ↓
Post-execution:
  ├─ update_global_error(task, response)  ← ALL HARDCODED VALUES
  ├─ calculate_reward(task, response)  ← HARDCODED quality 0.8
  ├─ DPO pair submission (rejected: String::new() — EMPTY)
  └─ distill_memory / crystallize_skills / soul_critique
```

**CRITICAL FINDING:** Tool dispatch uses `soul_tools::dispatch_tool()` → `execute_shell()` which calls bare `std::process::Command`. No sandbox in the dispatch path.

---

## 7. Critical Findings

### CRIT-001: Tool Dispatch Bypasses All Sandboxing
- **Severity:** Critical | **Confidence:** Confirmed
- **Category:** Security — Unrestricted Execution
- **Affected files:** `soul_tools/src/lib.rs:253-287,291-310`, `soul-agent-core/src/lib.rs:257,799-903`
- **Evidence:** `execute_shell()` at soul_tools:291 uses bare `std::process::Command`. `dispatch_tool()` at line 253 calls `execute_shell()` or direct `write_file()`/`read_file()` with no sandbox. The `AsyncShellExecutor` (sandboxed) is never called from the agent loop.
- **Impact:** Any LLM tool call results in unsandboxed shell execution
- **Recommended remediation:** Remove bare `Command` path, make all dispatch go through sandbox

### CRIT-002: soul_sandbox is String-Level Filtering, Not OS Sandboxing
- **Severity:** Critical | **Confidence:** Confirmed
- **Category:** Security — Insufficient Isolation
- **Affected files:** `soul_sandbox/src/lib.rs`, `soul_sandbox/src/policy.rs`
- **Evidence:** No namespace isolation, no bubblewrap, no Landlock, no cgroups, no rlimit. Path restrictions are string-based (`str::contains`). Seccomp is optional and defaults to "unconfined". No output size limits (OOM vector).
- **Impact:** Commands allowed by the sandbox have unrestricted filesystem, network, and resource access
- **Recommended remediation:** Integrate with bubblewrap/bound-system, add rlimits, make seccomp mandatory

### CRIT-003: No Authentication on Any Gateway, API, or Webhook Endpoint
- **Severity:** Critical | **Confidence:** Confirmed
- **Category:** Security — Missing Authentication
- **Affected files:** `soul_gateway/src/lib.rs`, `src/api.rs`, `src/ws_bridge.rs`, `clawd/src/lib.rs`
- **Evidence:** 13 REST routes, 16 API routes, WS bridge, all webhooks — zero auth middleware. TLS code exists but is dead code.
- **Impact:** Any process on the machine can execute shell commands, create PTYs, read/write memory, and manage agents
- **Recommended remediation:** Add Bearer token auth, enable TLS, make WS shared_secret mandatory

### CRIT-004: soul_tools Fallthrough Creates Arbitrary Code Execution
- **Severity:** Critical | **Confidence:** Confirmed
- **Category:** Security — Arbitrary Code Execution
- **Affected files:** `soul_tools/src/lib.rs:275`
- **Evidence:** `_ => execute_shell(&format!("{} {}", name, args))` — any unrecognized tool name becomes a shell command
- **Impact:** The LLM can call any unrecognized "tool name" and have it executed as a shell command
- **Recommended remediation:** Remove fallthrough, return error for unknown tools

### CRIT-005: Master Key and Secrets Never Zeroed
- **Severity:** Critical | **Confidence:** Confirmed
- **Category:** Security — Credential Exposure
- **Affected files:** `soullink-brain/soullink-secrets/src/crypto.rs:9-15`
- **Evidence:** `SecretsCrypto` stores `master_key: Vec<u8>` with no `Drop` implementation. `SecretValue` wraps `Vec<u8>` without `zeroize`. `secrecy` crate is declared but unused.
- **Impact:** Master key leakage can decrypt all stored secrets from process memory
- **Recommended remediation:** Use `zeroize::Zeroizing` or `secrecy::SecretBox`

### CRIT-006: Four Security Crates Are Completely Unreachable
- **Severity:** Critical | **Confidence:** Confirmed
- **Category:** Security — Dead Code / False Security
- **Affected files:** `soullink-brain/soullink-secrets/`, `soullink-brain/soullink-allowlist/`, `semantic_firewall/`, `soul_security/`, `soul_guard/`, `src/code_signing.rs`
- **Evidence:** Zero `use` references in any runtime code. `verify_code()` never called. These are registered workspace members with no dependents.
- **Impact:** Claims of code signing, secret management, firewall, intrusion detection are false
- **Recommended remediation:** Wire into runtime or remove and update documentation

### CRIT-007: Prompt Injection Filtering Is Applied After Memory Persistence
- **Severity:** Critical | **Confidence:** Confirmed
- **Category:** Security — Prompt Injection / Memory Poisoning
- **Affected files:** `soul-agent-core/src/lib.rs:524-551,475,1334-1410`, all memory `store()` functions
- **Evidence:** CCOS observation (line 868) and history recording (line 852) happen BEFORE `screen_tool_output()` (line 899). No memory crate scans for injection before persisting.
- **Impact:** Persistent memory poisoning, cross-session poisoning, instruction smuggling
- **Recommended remediation:** Move injection screening BEFORE all persistence paths

### CRIT-008: Simulated Autonomy in SoulEntity
- **Severity:** Critical | **Confidence:** Confirmed
- **Category:** Architecture — Misleading Implementation
- **Affected files:** `soul_entity/src/entity.rs:193-244,246-294,461-476`
- **Evidence:** `plan()` always generates 4 hardcoded steps. `execute_plan()` uses `format!("[OK] {}", step)`. Evaluation is always `score: 0.9` with feedback "simulation". Decision is always `action: "archive"`.
- **Impact:** The autonomous entity presents as functional but performs no real work
- **Recommended remediation:** Wire real `AutonomousAgent` delegation OR document as simulation

---

## 8. High Findings

### HIGH-001: Tool Dispatch and Agent Loop Both Bypass soul_sandbox
Same root cause as CRIT-001 — the `dispatch_tool()` → `execute_shell()` path uses bare `Command`.

### HIGH-002: write_file/patch_file Have No Path Restrictions
`soul_tools/src/lib.rs:338-358` — `std::fs::write(path, content)` with no canonicalization or allowlisting.

### HIGH-003: CCOS and Other Memory Uses Non-Atomic JSON Persistence
`ccos/src/external_memory.rs` writes `workspace.ccos` via `std::fs::write()` with no write-to-temp-then-rename pattern. Crash during write produces corruption.

### HIGH-004: Self-Modification Systems Can Modify Arbitrary Files Without Approval
`soul-automodify/src/lib.rs:101-160` writes to any `Path`. `soul-kernel/src/autocode/mod.rs:251-336` overwrites `.rs` files directly. All validation is bypassable.

### HIGH-005: openevolve auto_pr Pushes to External Repositories With Hardcoded Identity
`openevolve/src/auto_pr.rs:191-265` — creates git commits with hardcoded author, pushes branches. Score threshold of 0.8 is the only gate.

### HIGH-006: Two Telegram Bots Could Conflict
`clawd` and `soul_gateway/src/providers/telegram.rs` both use teloxide long-poll with the same `TELEGRAM_BOT_TOKEN`. If both start, they cause 409 CONFLICT.

### HIGH-007: soul-mcp Provides Unauthenticated Tool Execution Over WebSocket
`soul-mcp/src/lib.rs:651-668` — WebSocket MCP server with 5 tools including `execute_shell` and `write_file`, no auth.

### HIGH-008: soul-protocol UDP Discovery Broadcasts Agent Metadata on 0.0.0.0:42069
`soul-protocol/src/lib.rs:715-760` — responds to any `DISCOVER` UDP packet with full agent metadata. `send_to()` broadcasts to `255.255.255.255`.

### HIGH-009: Insecure "Backup Signing" Uses Symmetric HMAC Mistaken for Asymmetric
`src/backup.rs:62-72` — uses SHA-256 + HMAC-style symmetric, not `ed25519-dalek`. `generate_keys()` produces a random seed as "private key" and its SHA-256 as "public key".

### HIGH-010: All Webhook Verifications Are Lenient When Secrets Are Unset
`soullink-brain/soullink-gateway/src/channels/` — Discord/Slack/WhatsApp verification is lenient when env vars are empty.

---

## 9. Medium Findings

- **MED-001:** soul_llm has no retry logic — single HTTP attempt then error
- **MED-002:** soul_llm has no cancellation support — relies on Future drop
- **MED-003:** soul_llm has no rate limiting beyond token budget
- **MED-004:** Circuit breaker `with_service_name()` is a no-op
- **MED-005:** ToT uses placeholder embeddings (`vec![1.0_f32; 64]`)
- **MED-006:** DPO training collects no negative samples (`rejected: String::new()`)
- **MED-007:** Post-execution reward/penalty values are all hardcoded
- **MED-008:** RAG middleware initialized but never called in LLM pipeline
- **MED-009:** soul_entity module loader is unsafe with no trust verification
- **MED-010:** soul-wasm host functions are stubs (fd_write, proc_exit)
- **MED-011:** Multiple soul_tools functions are `#[allow(dead_code)]`

---

## 10. Low and Informational Findings

- **LOW-001:** soul_planner CognitiveLoop has no LLM integration (pure in-memory)
- **LOW-002:** model-router is only used by clawd, not the main agent
- **LOW-003:** scirust-gpu-macros `#[gpu]` is non-functional placeholder
- **LOW-004:** GPU backends in src/compute_backend.rs all delegate to CPU fallback
- **LOW-005:** soul_llm OpenAI/Anthropic providers lack native tool calling
- **LOW-006:** Two integration test crates have zero test source files
- **LOW-007:** check.sh has wrong hardcoded path (`/root/soul_system` vs `/root/SoulSystem`)
- **LOW-008:** Workspace lints silence dead_code, unused_imports, deprecated usage

---

## 11. Tool Capability Matrix

| Tool | Implementation | Side Effects | Capability | Approval | Sandbox | Path Restrictions | Output Filtering | Risk |
|------|---------------|-------------|------------|----------|---------|------------------|-----------------|------|
| `execute_shell` | soul_tools:291 | Shell command | Destructive | via caller | NONE | None | None | CRITICAL |
| `read_file` | soul_tools:320 | Read file | Read | via caller | NONE | None | None | MEDIUM |
| `write_file` | soul_tools:338 | Write file | Write | via caller | NONE | None | None | HIGH |
| `patch_file` | soul_tools:350 | Find-replace | Write | via caller | NONE | None | None | HIGH |
| dispatch_tool(fallback) | soul_tools:275 | name+args as command | Destructive | NONE | NONE | None | None | CRITICAL |
| soul_sandbox::execute | soul_sandbox | Sandboxed command | Write | None | String+seccomp | Sensitive paths | None | HIGH |
| bound-system::execute | bound-system:132 | Bubblewrap sandbox | Write | None | Bubblewrap+wl | Whitelist | None | MEDIUM |
| soul-mcp execute_shell | soul-mcp | Shell via MCP | Destructive | NONE | NONE | None | None | CRITICAL |
| soul-mcp write_file | soul-mcp | File via MCP | Write | NONE | NONE | None | None | HIGH |
| soul_gateway shell | soul_gateway | Shell via HTTP | Destructive | NONE | Depends | None | None | CRITICAL |
| API /api/exec | src/api.rs:160 | Shell via HTTP | Destructive | NONE | BoundSystem | Whitelist | None | HIGH |
| API /api/pty/* | src/api.rs | PTY sessions | Destructive | NONE | None | None | None | CRITICAL |
| API /api/memory/store | src/api.rs:265 | Write to memory | Write | NONE | N/A | None | None | HIGH |
| soul-automodify/modify | soul-automodify:101 | Write any file | Destructive | Optional flag | None | NONE | None | CRITICAL |
| openevolve auto_pr | openevolve/auto_pr | Git add/commit/push | Destructive | Score 0.8 | None | target_path | None | CRITICAL |
| AsyncShellExecutor | soul_tools:242 | Sandboxed command | Write | None | YES | Via sandbox | None | MEDIUM |
| soul-browser evaluate_js | soul-browser:331 | Arbitrary JS | Write | NONE | NONE | N/A | None | HIGH |

---

## 12. Network Endpoint Matrix

| Component | Route/Protocol | Auth | TLS | State-Changing | Risk |
|-----------|---------------|------|-----|----------------|------|
| soul_gateway | POST /v1/run | NONE | NONE | Shell execution | CRITICAL |
| soul_gateway | POST /v1/goal | NONE | NONE | Goal creation | HIGH |
| soul_gateway | POST /v1/cycle | NONE | NONE | Full cognitive cycle | HIGH |
| soul_gateway | POST /v1/stream (WS) | NONE | NONE | Event stream | MEDIUM |
| soul_gateway | POST /providers/*/webhook | Optional | NONE | LLM call | MEDIUM |
| src/api.rs | POST /api/exec | NONE | NONE | Shell execution | CRITICAL |
| src/api.rs | POST /api/pty/* | NONE | NONE | PTY control | CRITICAL |
| src/api.rs | POST /api/memory/store | NONE | NONE | Memory manipulation | HIGH |
| src/ws_bridge | WebSocket | Optional secret | NONE | Bus pub/sub | HIGH |
| soul-protocol | UDP discovery :42069 | NONE | N/A | Metadata leak | MEDIUM |
| clawd | Telegram long-poll | Bot token | TG TLS | Shell, PTY, Memory | CRITICAL |
| soul-kernel | POST /command :9051 | NONE | NONE | Goal/action injection | HIGH |
| soul-kernel | POST /inject :9051 | NONE | NONE | Code injection | CRITICAL |
| soul-mcp | WebSocket MCP | NONE | NONE | Tool execution | CRITICAL |
| soul_gateway | POST /providers/*/webhook | Optional | NONE | LLM call | MEDIUM |

---

## 13. Memory Subsystem Matrix

| Subsystem | Storage | Live Integration | Provenance | Deterministic | Injection Filtering | Transactional | Recommendation |
|-----------|---------|-----------------|------------|---------------|-------------------|---------------|---------------|
| soul-memory (store.rs) | Sled+Qdrant(mock) | soul-agent-core, souls | NONE | Yes | NONE | Per-key | PRESERVE |
| soul-memory (conversations.rs) | SQLite | soul-agent-core, souls | NONE | Yes | NONE | NO (2 stmts) | PRESERVE |
| soul-memory (graph.rs) | In-memory+JSON | soul-agent-core | NONE | Yes | NONE | No | PRESERVE |
| soul-memory (persist.rs) | Sled | soul-agent-core, souls | NONE | Yes | NONE | Per-key | PRESERVE |
| soul-memory (rag.rs) | In-memory cache | soul-rag | NONE | No | NONE | N/A | PRESERVE |
| soul_persistence | Redb | soul_entity, soul_repl | FULL (parent_id) | Yes | NONE | YES (redb txn) | PRESERVE |
| soul-compaction | In-memory | soul-agent-core | N/A | Yes | N/A | N/A | PRESERVE |
| ccos | JSON files | soul-agent-core, soul_cognitive | FULL (hash chain) | Yes | NONE | NO (non-atomic) | PRESERVE |
| soul-graph-memory | Sled(unused)+JSON | NONE | NONE | Yes | NONE | No | REMOVE |
| soul-conversations | SQLite | NONE | NONE | Yes | NONE | No (2 stmts) | REMOVE |
| soul-persist | Sled | NONE | NONE | Yes | NONE | Per-key | REMOVE |
| soul_journal | Mmap file | soul_entity | NONE (bytes) | N/A | N/A | YES (CAS) | PRESERVE |
| soullink-memory | Sled+HNSW | soullink-autonomy, etc. | NONE | No | NONE | Per-key | PRESERVE |
| soullink-memory-hierarchy | In-memory only | soul_entity, soul-agent-core | NONE | Yes | NONE | No | PRESERVE |
| soul-designtree | JSON files | NONE | NONE | Yes | NONE | No | PRESERVE |

---

## 14. Component Disposition

| Component | Preserve | Merge | Deprecate | Remove |
|-----------|----------|-------|-----------|--------|
| soul-agent-core | ✓ | — | — | — |
| soul_entity | — | — | ✓ | ✓ |
| soul-kernel | — | ✓ | — | — |
| souls | ✓ | — | — | — |
| soul_repl | ✓ | — | — | — |
| soul-daemon | — | ✓ | — | — |
| soul_llm | ✓ | — | — | — |
| soul_tools | — | — | ✓ | ✓ (rewrite) |
| soul_sandbox | — | ✓ | — | — |
| bound-system | ✓ | — | — | — |
| soul-memory | ✓ | — | — | — |
| soul_persistence | ✓ | — | — | — |
| soul-compaction | ✓ | — | — | — |
| ccos | ✓ | — | — | — |
| soul_journal | ✓ | — | — | — |
| soul-graph-memory | — | — | — | ✓ |
| soul-conversations | — | — | — | ✓ |
| soul-persist | — | — | — | ✓ |
| soul_gateway | ✓ | — | — | — |
| soul-mcp | ✓ | — | — | — |
| soul-protocol | ✓ | — | — | — |
| soullink-circuit | ✓ | — | — | — |
| soullink-gate | ✓ | — | — | — |
| soullink-security | ✓ | — | — | — |
| soullink-secrets | ✓ | — | — | — |
| soullink-allowlist | ✓ | — | — | — |
| semantic_firewall | ✓ | — | — | — |
| soul_security | ✓ | — | — | — |
| soul_guard | ✓ | — | — | — |
| code_signing | ✓ | — | — | — |
| soul-automodify | ✓ | — | — | — |
| openevolve | — | ✓ | — | — |
| scirust-core | ✓ | — | — | — |
| scirust-simd | ✓ | — | — | — |
| soul-wasm | ✓ | — | — | — |
| clawd | ✓ | — | — | — |
| soul_api | — | — | ✓ | — |
| soul-bridge | ✓ | — | — | — |
| soul-eventbus | ✓ | — | — | — |
| soul-skills | ✓ | — | — | — |
| os-agents/ | — | — | — | ✓ |
| soullink-node/ | — | — | — | ✓ (separate) |

---

## 15. OpenClaw and Hermes-Agent Comparison

| Capability | SoulSystem (Current) | OpenClaw | Hermes-Agent |
|------------|---------------------|----------|-------------|
| Installation | curl pipe, npm, cargo (3 methods) | curl pipe | pip install |
| Canonical CLI | NONE — 3+ runtimes | Single `openclaw` | `hermes` |
| Canonical Runtime | NONE — 3 competing | Single runtime | Single runtime |
| Sandboxing | String-level + unconnected bubblewrap | Container sandbox | Docker |
| Approval | ApprovalGate exists but dispatch bypasses | Required for dangerous ops | Required |
| Security Documentation | MISLEADING — claims unconnected features | Documented | Documented |
| Operational Maturity | Pre-production | Production-ready | Production-ready |

**Verdict:** SoulSystem is not currently a competitor. It has more ambitious scope but lacks basic runtime coherence and security enforcement.

---

## 16. Threat Model

### 16.1 Attack Scenarios

**A. Persistent Memory Poisoning via Tool Output**
1. Attacker places malicious file on a website the agent visits
2. Tool output enters CCOS memory (line 868), planner history (line 852), semantic memory (line 475) BEFORE injection screening (line 899)
3. In subsequent sessions, retrieved memory contains the injection payload

**B. Gateway Takeover via Unauthenticated Endpoint**
1. Any process connects to :9023
2. POST /api/exec executes arbitrary shell commands
3. POST /api/memory/store writes arbitrary content to memory

**C. Self-Modification Pipeline Hijack**
1. LLM generates code modification via crystallize_skills()
2. Code passes through soul-automodify/modify() with validate: false
3. Arbitrary .rs files are overwritten

**D. Supply Chain via Dependency**
1. CI does not run cargo deny or cargo audit
2. Dependency with known CVE is introduced (wildcards allowed)
3. The dependency is used in a reachable code path

---

## 17. Scientific-Code Readiness

### Genuinely Implemented
- **scirust-core**: ~45K LOC deep learning framework with autograd, 54+ NN modules, transformers, optimizers, quantization, quantum MPS, homomorphic encryption
- **scirust-simd**: Runtime-dispatched SIMD (AVX2/SSE2/NEON/SVE), bit-exact INT4 dequant
- **scirust-symbolic**: Symbolic math with parser, simplifier, diff, solver, code generation
- **scirust-learning**: Linear/polynomial regression, pattern discovery

### Missing for Scientific Readiness
- Deterministic numerical execution (no global seed, HashMap-based grading)
- Reproducible experiments (no manifest format, no seed management)
- Dataset provenance (no hashing or tracking)
- GPU backends (all stubs that delegate to CPU)
- Property-based testing (no proptest infrastructure)
- Differential testing (none)
- Benchmark capture (only one bench in CI)

**Verdict:** The ML framework exists but the scientific workflow tooling does not. Claim of "scientific code specialization" is premature.

---

## 18. Industrial-Operation Readiness

### What Exists
- Watchdogs (SelfHealer, clawd-supervisor)
- Crash recovery (soul-daemon checkpoint/rollback)
- Graceful shutdown (Ctrl+C handlers)
- Action security validation (soul-kernel is_safe_* functions)

### What Is Missing
- Idempotent operations (no operation IDs, no dedup)
- Transactional commands (no command ack/nack protocol)
- Offline operation (all systems assume network+Ollama)
- Human approval (ApprovalGate exists but dispatch bypasses it)
- Emergency stop (soul_guard is dead code)
- RBAC (no role or user concept)
- Signed policies (none)
- Deterministic scheduling (no real-time guarantees)

**Verdict:** Not safe for industrial operations. SelfHealer can restart services, kernel can modify iptables, auto-evolution can modify source code — all without human approval or safety interlocks.

---

## 19. Performance Readiness

### Known Anti-Patterns
- Unbounded String reads in sandbox (OOM vector)
- Per-request process spawning (no persistent worker pool)
- Missing backpressure in most channels
- No latency budgets or SLAs

### Benchmark Plan (Required for 1.0)

```
Benchmark                    Target
─────────────────────────────────────────────
Cold startup                 < 500ms
First-token latency          < 500ms
Streaming throughput          > 50 tok/s
Tool dispatch latency        < 10ms
Sandbox startup               < 50ms
Causal-memory insert          < 1ms
Causal-memory retrieval       < 5ms
Event replay (100K)           < 5s
Concurrent sessions (10)     < 500MB
Gateway throughput (100/s)    < 1s p99
Scientific kernel (matmul)   < 10ms
```

**Verdict:** Performance has not been measured. Claims of "high-performance" are unsupported.

---

## 20. Target Architecture

### Key Design Decisions
1. **One CLI** — `souls` becomes the canonical CLI, `soulsystem` is deprecated
2. **One runtime** — `AutonomousAgent` from soul-agent-core becomes canonical
3. **One tool registry** — soul_tools rewritten: explicit typed, no fallthrough, sandbox inline
4. **One sandbox** — bound-system (bubblewrap) mandatory on Linux; soul_sandbox as non-Linux fallback
5. **One memory entry** — soul-memory as unified facade with soul_persistence, ccos, soul_journal as backends
6. **One gateway** — soul_gateway with mandatory auth, TLS, rate limiting

---

## 21. Immediate Containment (Phase 0)

| # | Action | Rationale |
|---|--------|-----------|
| 0.1 | Disable soul_gateway, HTTP API, WS bridge by default | No auth on any endpoint |
| 0.2 | Remove dispatch_tool fallthrough | Arbitrary code execution |
| 0.3 | Add execute_shell to banned-by-default | Most dangerous tool |
| 0.4 | Gate all self-modification behind explicit config | Prevent unintended code changes |
| 0.5 | Disable soul-protocol UDP discovery | Leaks agent metadata |

---

## 22. Phased Roadmap

### Phase 1 — Canonical Runtime (Weeks 1-4)
Establish soul-agent-core::AutonomousAgent as the single canonical runtime. Replace SoulEntity simulation. Merge soul-daemon checkpointing.

### Phase 2 — Capability Security (Weeks 2-6)
Typed capability registry, mandatory sandbox enforcement in all dispatch paths, output size limits, MCP auth.

### Phase 3 — Network Security (Weeks 4-8)
Mandatory Bearer token + TLS on all endpoints, mandatory webhook signatures, disable UDP discovery.

### Phase 4 — Memory Security (Weeks 6-10)
Injection scanning before all persistence, quarantine mechanism, transactional writes.

### Phase 5 — Scientific Execution (Weeks 8-16)
Deterministic execution, experiment manifests, dataset provenance.

### Phase 6 — Industrial Operations (Weeks 12-20)
Wire emergency stop, add safe-state machine, idempotent actions, RBAC.

### Phase 7 — Product Maturity (Ongoing)
Unified CLI, documentation, CI, packaging, release integrity.

---

## 23. First Ten Pull Requests

### PR #1: Route All Tool Execution Through Sandbox
- Remove bare `std::process::Command` from soul_tools
- Make all dispatch go through AsyncShellExecutor → Sandbox::execute()
- Add output size limits (100KB)
- Add output timeout enforcement

### PR #2: Mandatory OS-Level Sandbox via Bound-System
- Delegate soul_sandbox to bound-system on Linux
- Bubblewrap with namespace isolation, seccomp, rlimits
- Network isolation, output size limits

### PR #3: Mandatory Authentication on All Network Endpoints
- Bearer token middleware on all gateway routes
- TLS support in soul_gateway serve()
- Webhook verification fail-closed when secrets unset
- Disable UDP discovery by default

### PR #4: Zeroize Secrets and Fix Code Signing
- Use zeroize for all secret storage
- Replace HMAC backup "signing" with real ed25519
- Fix or remove custom base64 implementation

### PR #5: Wire Security Crates Into Runtime
- Wire soullink-allowlist into URL dispatch
- Wire soul_security RateLimiter into gateway
- Wire soul_guard as emergency stop
- Wire semantic_firewall into memory pipeline
- Wire code_signing into self-modification

### PR #6: Enforce Injection Filtering Before Memory Persistence
- Reorder run_react() to scan BEFORE persistence
- Add InjectionScanner to all store() implementations
- Quarantine mechanism for suspicious content

### PR #7: Replace SoulEntity Simulation With Real AutonomousAgent
- Delegate plan/execute/evaluate/decide to AutonomousAgent
- Deprecate run_cycle() in favor of run_task()

### PR #8: Merge soul-daemon Into soul-agent-core
- Port checkpointing, rollback, stall detection
- Add AgentConfig options for checkpoint interval, stall timeout

### PR #9: Merge soul-kernel Actions Into soul-agent-core
- Port Action enum variants (RestartService, BlockIp, etc.)
- Port resilience engine and security validation functions

### PR #10: Remove Abandoned Duplicate Crates
- Remove soul-graph-memory, soul-conversations, soul-persist
- Remove os-agents/ directory
- Remove ccos/CCOS/, empty integration_tests/
- Remove neural-store/, jit-agentic-engine/

---

## 24. CI and Validation Matrix

| Requirement | Current | Target (v1.0) |
|-------------|---------|---------------|
| Workspace check | ✓ | ✓ |
| Full test suite | ✗ Skips soul-kernel | ✓ All crates |
| Integration tests | ✗ One test | ✓ E2E agent test |
| Clippy -D warnings | ✓ (with allowlist) | ✓ No allowlist |
| MSRV check | ✗ | ✓ |
| Feature combinations | ✗ | ✓ |
| Platform matrix | ✗ Linux only | ✓ Linux+macOS |
| cargo deny check | ✗ Config unused | ✓ Run in CI |
| cargo audit | ✗ Config unused | ✓ Run in CI |
| Fuzzing | ✗ Targets defined, never run | ✓ Run in CI |
| Miri | ✗ | ✓ For unsafe code |
| Sanitizers | ✗ | ✓ ASan, TSan |
| Performance regression | ✗ Placeholder | ✓ Benchmark compare |
| Docker build | ✗ Not tested | ✓ Container build+test |
| SBOM | ✗ | ✓ With releases |

---

## 25. Final Decision

### 1. Is SoulSystem currently a production-ready competitor to OpenClaw?
**NO.** Lacks runtime coherence, has simulated autonomy, bypassed security, no auth on any endpoint.

### 2. Is SoulSystem currently a production-ready competitor to Hermes-Agent?
**NO.** Lacks tool ecosystem, documentation, production stability.

### 3. What is genuinely implemented?
scirust-core (45K LOC ML framework), scirust-simd (SIMD dispatch), soul-agent-core AutonomousAgent (real ReAct loop with circuit breaker, gate, injection scan), soul_persistence (provenance), soul_journal (WAL), ccos (causal memory), soullink-circuit/gate (security), soul-protocol (A2A), soul-mcp (MCP), bound-system (bubblewrap), SoulLink neural mesh (40+ crates).

### 4. What is genuinely differentiated?
Pure Rust, CCOS causal memory, scientific computing base in same repo, WASM plugin runtime, lock-free mmap WAL.

### 5. What is simulated?
SoulEntity autonomous loop (hardcoded plans/fake execution/always-0.9 evaluation), soul_repl /plan/run/observe commands, post-execution rewards (all hardcoded), ToT embeddings (placeholder), DPO training (no negative samples), GPU backends (all CPU fallback), anomaly HNN tick rate (simulated counter).

### 6. What is bypassed?
Tool dispatch bypasses sandbox (bare Command), dispatch bypasses authorization (no auth in dispatch_tool), memory persistence bypasses injection filtering (stored before scan), self-modification validation optional (validate: false bypassable), gateway TLS bypassed (TlsConfig unused).

### 7. What is fragmented?
3 competing agent runtimes, 13 memory crates (3 duplicates), 2 sandbox systems, 2 Telegram bots, 3 webhook implementations, 2 empty integration test crates, 56+ binaries.

### 8. What is unsafe?
Tool dispatch via bare Command (CRIT-001), string-level sandbox bypassable (CRIT-002), no auth on any network endpoint (CRIT-003), fallthrough creates arbitrary code execution (CRIT-004), secrets never zeroed (CRIT-005), 4 security crates dead code (CRIT-006), memory poisoning before injection filtering (CRIT-007).

### 9. Which runtime must become canonical?
**soul-agent-core::AutonomousAgent** — most complete: circuit breaker, ApprovalGate, InjectionScanner, CCOS, three planning strategies, clean trait-based architecture.

### 10. Which components must be preserved?
soul-agent-core, soul_llm, soul-memory, soul_persistence, soul-compaction, ccos, soul_journal, soullink-circuit, soullink-gate, scirust-core, scirust-simd, soul_gateway, soul-mcp, soul-protocol, soul_repl, souls, clawd, soul-eventbus, soul-skills, soul-subagents, soul-goaltree, soul-designtree.

### 11. Which components must be merged?
soul-daemon → soul-agent-core, soul-kernel actions → soul-agent-core, soul-bridge → soul_gateway, soul-graph-memory → soul-memory, soul-conversations → soul-memory, soul-persist → soul-memory.

### 12. Which components must be deprecated?
soul_entity, soul_api, soul_security (merge into gateway), soul_guard (merge into runtime), soul-wasm (document as incomplete).

### 13. What are the five immediate security corrections?
(1) Disable all network endpoints by default. (2) Remove dispatch_tool fallthrough. (3) Ban execute_shell by default. (4) Gate all self-modification behind explicit flags. (5) Disable UDP discovery.

### 14. What development activity should stop immediately?
New agent runtimes, new memory crates, new webhook implementations, maintaining os-agents/ duplicates.

### 15. What should be built next?
PR #1 (sandbox enforcement), PR #3 (network auth), PR #10 (remove duplicates), PR #7 (real autonomy).

### 16. Should the repository be repaired incrementally or reorganized through staged migration?
**Incremental repair.** Each PR must leave the system functional. No "big bang" rewrite.

### 17. What measurable conditions must be met before version 1.0?
See Section 27 definitions: security (8 pass/fail), runtime unification (4), memory (4), CI (8), documentation (4).

### 18. What remains UNVERIFIED?
Performance metrics, GPU sub-workspace compilability, soul-neural crate contents, soul-cognition sub-module internals, soul-kernel Q-learning convergence, external service availability, soul-wasm host function correctness, full integration test with real Ollama, memory consumption under load.
