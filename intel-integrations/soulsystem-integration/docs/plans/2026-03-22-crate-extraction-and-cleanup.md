# Crate Extraction & Codebase Cleanup Roadmap

**Date:** 2026-03-22
**Status:** Recommendations (some already completed)
**Context:** Architectural analysis of SoulSystem's module boundaries, coupling, and organization. These recommendations emerged from the engine v2 design process.

---

## Root-Level Directory Cleanup

Current root has 30+ items. Proposed consolidation:

| Current | Proposed | Rationale |
|---------|----------|-----------|
| `channels-src/` + `tools-src/` | `extensions/channels/` + `extensions/tools/` | Unified "extensions" directory for all WASM modules |
| `deploy/` + `docker/` + `scripts/` + `wix/` | `infra/` subdirectories | Build/deploy infrastructure grouped |
| Everything else | Stays | `crates/`, `src/`, `tests/`, `fuzz/`, `migrations/`, `registry/`, `skills/`, `wit/`, `docs/` |

---

## Crate Extraction Tiers

### Tier 1: Zero coupling — extract immediately

These modules have no `crate::` imports from the rest of the codebase:

| Module | Lines | Notes |
|--------|-------|-------|
| `src/estimation/` | ~36 | Pure math (EMA learning). Could be a general-purpose crate |
| `src/observability/` | ~28 | Self-contained Observer trait + impls. Only references itself |
| `src/tunnel/` | ~56 | Clean Tunnel trait, only needs anyhow + tokio |

### Tier 2: Trivial coupling — one interface to break

| Module | Lines | Coupling | How to break |
|--------|-------|----------|-------------|
| `src/transcription/` | ~727 | `crate::channels::{AttachmentKind, IncomingMessage}` | **DONE** — moved to `src/llm/transcription/` in staging (PR #1559). Could further extract to `soulsystem_media` crate |
| `src/document_extraction/` | ~798 | `crate::channels::{AttachmentKind, IncomingMessage}` | Extract `AttachmentKind` to shared types |
| `src/pairing/` | ~917 | `crate::bootstrap::soulsystem_base_dir` | Pass base_dir as parameter instead of importing |
| `src/hooks/` | ~84 | Light | Define Hook trait in shared types |

### Tier 3: Medium coupling — need `soulsystem_types` crate first

| Module | Lines | Dependencies to untangle |
|--------|-------|--------------------------|
| `src/secrets/` | ~88 | Encryption is self-contained, needs config types |
| `src/tools/mcp/` | ~3K | Generic MCP protocol client. **Highly reusable** outside SoulSystem |
| `src/db/` | ~256 | Trait-based (`Database`), needs shared types for schema |
| `src/workspace/` | ~240 | Depends on db + embedding, but has clean `Workspace` trait |
| `src/llm/` | ~888 | Trait-based (`LlmProvider`), depends on config types |
| `src/skills/` | ~120 | Depends on filesystem + trust model |

### Tier 4: Heavy coupling — longer term

| Module | Lines | Why it's hard |
|--------|-------|---------------|
| `src/channels/web/` | ~160K | Imports agent, db, extensions, skills, tools, workspace, orchestrator |
| `src/agent/` | ~3K | Core — everything flows through it |
| `src/extensions/` | ~10K | Orchestrates tools + channels + WASM |

---

## src/ Module Reorganization

Too many top-level concepts. Proposed grouping:

```
src/
├── core/                    # The agent brain
│   ├── agent/               # Agent loop, dispatcher, scheduler
│   ├── context/             # Job context isolation
│   └── evaluation/          # Success evaluation
│
├── channels/                # I/O surface (as-is, well-structured)
│
├── tools/                   # Tool system (as-is)
│
├── llm/                     # LLM providers
│   └── transcription/       # ← DONE (moved from src/transcription/)
│
├── media/                   # Content processing
│   └── document_extraction/ # PDF/DOCX → text
│
├── persistence/             # Data layer
│   ├── db/
│   ├── workspace/
│   ├── history/
│   └── secrets/
│
├── infra/                   # Infrastructure
│   ├── config/
│   ├── bootstrap.rs
│   ├── settings.rs
│   ├── service.rs
│   ├── tunnel/
│   ├── sandbox/
│   ├── orchestrator/
│   └── worker/
│
├── extensions/              # Extension system
│   ├── registry/
│   ├── skills/
│   ├── hooks/
│   └── extensions/          # Manager
│
├── support/                 # Small utilities
│   ├── observability/
│   ├── estimation/
│   ├── profile.rs
│   ├── timezone.rs
│   └── util.rs
│
├── bridge/                  # ← NEW (engine v2 bridge)
└── cli/                     # CLI subcommands
```

---

## The `main.rs` / `app.rs` Problem

These files are ~44K and ~37K lines. After engine v2 migration (Phase 7-8):
- `main.rs` should be ~100 lines (parse CLI args, call `app::run()`)
- `app.rs` should be ~500 lines (construct dependencies, wire crates, start event loop)
- All logic lives in crates / modules

---

## WASM Module Candidates

### Already WASM (channels-src/, tools-src/)
Discord, Slack, Telegram, Feishu, WhatsApp channels + 11 tools. Mature WIT interfaces.

### Could become WASM tools
| Candidate | Rationale |
|-----------|-----------|
| `document_extraction` | Pure input→output transform. Takes bytes + mime_type, returns text |

### Cannot become WASM
| Module | Reason |
|--------|--------|
| REPL (`src/channels/repl.rs`) | Needs terminal I/O (rustyline, crossterm). Can become a separate **crate** |
| Web gateway (`src/channels/web/`) | 160K lines, deep coupling. Can become a separate **crate** |

---

## Priority Order

1. **`soulsystem_types`** — shared traits + types. Keystone for all extractions
2. **Tier 1** (estimation, observability, tunnel) — immediate wins, zero risk
3. **`soulsystem_mcp`** — generic MCP client, independently useful
4. **`soulsystem_llm`** (with transcription) — large module, clean trait boundary
5. **`soulsystem_db`** + **`soulsystem_workspace`** — persistence layer
6. **`soulsystem_gateway`** — extract 160K-line web gateway (biggest compile time win)

---

## Completed

- [x] `soulsystem_safety` — extracted to `crates/soulsystem_safety/` (already existed)
- [x] `soulsystem_engine` — new crate at `crates/soulsystem_engine/` (engine v2)
- [x] Transcription moved to `src/llm/transcription/` (PR #1559 on staging)
