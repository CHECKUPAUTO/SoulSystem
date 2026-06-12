# Codex Harness Integration Guide

**Based on OpenEvolve Night Cycle Analysis**  
**Generated:** 2026-04-11  
**Source Report:** night_cycle_20250411_0719.md

---

## Overview

OpenClaw has added significant Codex (OpenAI Codex CLI) integration:

- **Pluggable Agent Harness Registry** - Runtime selection of agent backends
- **App-Server Controls** - Codex server management
- **Strict-Agentic Execution Contract** - Security and predictability boundaries

---

## Components

### 1. Pluggable Harness Registry

```typescript
// Harness registration
interface AgentHarness {
  name: string;
  supportedModes: ExecutionMode[];
  spawn(options: SpawnOptions): AgentSession;
}

// Registration
harnessRegistry.register('codex', new CodexHarness());
```

### 2. Strict-Agentic Execution Contract

**Purpose:** Defines clear boundaries between agent execution modes

```typescript
// Strict mode enforces:
// - No shell escapes
// - Explicit permission model
// - Sandboxed execution
// - Audit logging
```

### 3. App-Server Controls

```bash
# Start Codex app server
openclaw codex server start

# Check version compatibility
openclaw codex server version
```

---

## Security Model

- **Scope-aware failures** - Auth failures gated to Codex
- **Auth classification** - Clear error categorization
- **Runtime isolation** - Agent sessions isolated from gateway

---

## Integration Points

1. **Session Spawning** - `sessions_spawn` with runtime="acp"
2. **Thread Binding** - Discord thread persistence
3. **PTY Support** - Full terminal UI for Codex

---

## Recommendations

### Immediate
- [ ] Validate Codex harness with integration tests
- [ ] Document agent harness API for skill developers

### Short-term
- [ ] Add harness benchmark suite
- [ ] Document strict-agentic contract for contributors

### Long-term
- [ ] Consider API versioning for harness registry
- [ ] Support additional agent backends (Claude Code, etc.)

---

## References

- Commits: `31a0b7bd42`, `dd26e8c44d`, `44ec4d05de`
- Related: `09b1117271` - Strict-agentic execution contract
