# OpenClaw Core Rust Migration Roadmap

**Source:** Night cycle reports 2026-04-14 (00:00 through 05:01)
**Created:** 2026-04-14 by auto-apply
**Status:** Reference documentation (no code changes — requires approval for implementation)

---

> ⚠️ **This document is a roadmap only.** All OpenClaw core code changes require explicit human approval per auto-apply policy.

---

## Current State

| Domain | TS Files | Rust LOC | Migration % |
|--------|----------|----------|-------------|
| OpenClaw packages | 11,071 | 7 (IronReview stub) | 3.3% |
| rust-migration/ | — | 2,871 (iron-review + open-evolve) | Complete |
| soullink-* crates | 0 | 2,684 | Active |
| soullink_brain/ (nvme) | 0 | 825 | Active |
| **Total Rust** | — | **6,380 LOC** | — |

---

## Migration Priority Queue (OpenClaw Core)

### Phase 1: Pure-Function Modules (Low Risk, High Foundation Value)

| Priority | TS Module → Rust Crate | Files | Rationale |
|----------|----------------------|-------|-----------|
| P0 | `config` → `openclaw-config` | 306 | Pure parsing/validation, no I/O, foundation for everything |
| P1 | `cron` → `openclaw-cron` | 133 | Tokio scheduler, clear interface, self-contained |
| P2 | `security` → `openclaw-security` | 77 | Pure validation, audit rules, no runtime deps |
| P3 | `secrets` → `openclaw-secrets` | 100 | Key management, RocksDB/encrypted storage |
| P4 | `shared` → `openclaw-shared` | 89 | Utility functions, used everywhere |
| P5 | `process` → `openclaw-process` | 29 | Process management, tokio::Command wrapper |
| P6 | `sessions` → `openclaw-sessions` | 18 | Session store, RocksDB, self-contained |

### Phase 2: I/O-Bound Modules (Medium Risk)

| Priority | TS Module → Rust Crate | Rationale |
|----------|----------------------|-----------|
| P7 | `sl-gateway-core` | Every request passes through this. 3-5x throughput. Stack: axum + tower |
| P8 | `sl-web-fetch` | Most frequently called tool, I/O-bound, perfect for Rust. Stack: reqwest + scraper |
| P9 | `sl-context-engine` | Critical path for every LLM call, complex tree operations. Stack: im (immutable) |
| P10 | `sl-session-store` | 5x faster session ops. Stack: dashmap + rocksdb |
| P11 | `sl-mcp-protocol` | Type-safe protocol handling, zero-copy. Stack: serde + tokio channels |

### Migration Approach
1. Create Rust crate with FFI-compatible types (serde shared schemas)
2. Port pure functions first (parsing, validation, transformation)
3. Add axum/CLI layer that calls the Rust crate via `napi-rs` or CLI binary
4. Run TS and Rust in parallel for validation
5. Switch over when parity confirmed

---

## soullink-server-core: Shared Library Proposal

**Problem:** Every organ node re-implements axum + RocksDB + health check boilerplate (~300 LOC per organ).

**Solution:** Extract common patterns into `soullink-server-core` lib crate.

### Contents
- axum server setup + graceful shutdown
- RocksDB initialization + column families helper
- `/health` endpoint pattern (returns `{"ok": true, "version": "..."}`)
- Configuration from environment variables
- Logging/tracing setup
- Error types and JSON response helpers
- Node registration with orchestrator

### Impact
- Cut new organ implementation time by ~60%
- Standardize health checks, error handling, and shutdown across all organs
- Estimated: 300-400 LOC, 1-2 days to implement

---

## Gateway WS 1006 Issue

**Persistent issue across all reports.** Gateway daemon running, RPC OK, but WebSocket layer broken.

### Root Cause Analysis
- Environment variable: `OPENCLAW_GATEWAY_URL=ws://127.0.0.1:18889/ws`
- Gateway listening on port 18890
- **Mismatch**: WS client connects to 18889, gateway on 18890

### Proposed Fix
- Either update env var to port 18890, or update gateway config to listen on 18889
- Quick test: `openclaw gateway restart` (may resolve stale connections)

> ⚠️ This is a core code change — requires approval.

---

## Gateway Rust Rewrite (Long-term)

**Current:** Node.js/Express with persistent WS 1006 issues
**Target:** Axum + tokio-tungstenite native Rust server

### Rationale
- Eliminate WS 1006 closure
- Improve performance 10-50x
- Unified Rust stack across brain + gateway

### Estimated Timeline
- 6-8 cycles (complex, needs careful API compatibility)
- Risk: HIGH — gateway is the most critical component

### Phased Approach
1. Create `soullink-gateway` crate with WebSocket handler
2. Port session middleware (sl-session-store first)
3. Port routing layer
4. Port plugin system
5. Parallel run both gateways
6. Cut over

> ⚠️ This is a core code change — requires explicit approval.

---

## Unified Cargo Workspace

**Proposal:** Merge all `soullink-*` crates into a single Cargo workspace for unified builds.

### Current State
- Multiple independent Cargo.toml files
- Each built separately
- No shared dependency management

### Target State
```
soullink-workspace/
├── Cargo.toml (workspace)
├── crates/
│   ├── soullink-node/       (core node library)
│   ├── soullink-orchestrator/
│   ├── soullink-server-core/ (shared patterns)
│   ├── soullink-memory/
│   ├── soullink-reflex/
│   ├── soullink-synthesis/
│   ├── soullink-decision/
│   ├── soullink-market/
│   ├── soullink-critic/
│   └── ...
└── targets/
```

### Benefits
- Unified `cargo build --workspace`
- Shared dependency versions
- Faster incremental builds
- Cross-crate type sharing

**Estimated:** 3 hours to set up, low risk.

---

## Metrics Summary

| Metric | Current | 3-Cycle Target | 6-Cycle Target |
|--------|---------|---------------|---------------|
| Rust crates | 12 | 16 | 20 |
| Rust LOC | 6,380 | 10,000+ | 15,000+ |
| Brain organs active | 6 | 8 (+Memory, +Reflex) | 10 (+Synthesis, +Decision) |
| Python processes | 4 | 2 | 0 |
| Discovered attractors | 1 | 5 | 8+ |
| Migration % (brain stack) | 63.6% | 82% | 100% |
| Migration % (OpenClaw core) | 3.3% | 5% | 10% |