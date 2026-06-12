# Three-Plane Architecture Proposal

**Source:** OpenEvolve Night Cycle Analysis (night_cycle_20260411_0145.md)  
**Generated:** 2026-04-11 01:49 UTC  
**Status:** Reference Documentation - Ready for Implementation

---

## Overview

This document outlines a proposed architectural refactoring for OpenClaw's Gateway component, moving from a monolithic embedded architecture to a three-plane separation of concerns.

## Current Architecture Issues

**Coupling Score:** 7/10 (high)  
**Cohesion Score:** 6/10 (medium)

**Current Problems:**
- Gateway embeds plugin lifecycle, auth, sessions, cron, discovery, channels
- Global singleton state for registries
- Session storage is file-system based
- Subsystem loggers indicate tight coupling
- Canvas/UI embedded in gateway process

## Proposed Three-Plane Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                      CONTROL PLANE                              │
├─────────────────────────────────────────────────────────────────┤
│  Session Manager  │  Policy Engine  │    Orchestration        │
│  (stateless)      │  (policy-as-code)│    (workflow engine)     │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                       DATA PLANE                                │
├─────────────────────────────────────────────────────────────────┤
│  Channels         │  Tool Execution  │     Canvas/UI            │
│  (25+ platforms)  │  (sandboxed)     │     (isolated)           │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                      COMPUTE PLANE                              │
├─────────────────────────────────────────────────────────────────┤
│              AI Model Gateway (stateless, scalable)             │
│         (OpenAI, Claude, Gemini, Local Models)                  │
└─────────────────────────────────────────────────────────────────┘
```

### Control Plane Responsibilities

**Session Manager:**
- Stateless session routing
- Activation mode handling
- Session lifecycle management
- No direct tool/channel access

**Policy Engine:**
- Security audit as policy-as-code
- Mutation guards
- Rate limiting rules
- Configuration validation

**Orchestration:**
- Workflow engine for complex tasks
- DAG execution
- State machine management
- Recovery handling

### Data Plane Responsibilities

**Channels:**
- 25+ platform adapters
- Message normalization
- Media pipeline handling
- Platform-specific logic isolated

**Tool Execution:**
- Sandboxed execution
- Resource limits
- Timeout handling
- Audit logging

**Canvas/UI:**
- Visual workspace
- A2UI components
- Agent-driven rendering
- Isolated from core gateway

### Compute Plane Responsibilities

**AI Model Gateway:**
- Stateless request routing
- Model provider abstraction
- Token management
- Cost tracking

## Migration Path

### Phase 1: Decompose Gateway (Weeks 1-2)
- Extract SessionManager to separate plane
- Create PolicyEngine abstraction
- Maintain backward compatibility

### Phase 2: Process Isolation (Weeks 3-4)
- Resource-based isolation instead of session-based
- Container/Docker sandboxing
- IPC between planes

### Phase 3: Storage Abstraction (Weeks 5-6)
- SessionStore interface (file/Redis/PostgreSQL)
- TaskStore for cron jobs
- Migration utilities

## Benefits

1. **Scalability:** Each plane can scale independently
2. **Reliability:** Isolated failures don't cascade
3. **Security:** Clear trust boundaries
4. **Maintainability:** Smaller, focused components
5. **Testability:** Each plane testable in isolation

## Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Breaking changes | Medium | High | Gradual migration with feature flags |
| Performance regression | Low | Medium | Benchmark each phase |
| Complexity increase | Medium | Low | Clear documentation, gradual rollout |

## References

- Source: OpenEvolve Night Cycle Report 20260411_0145
- Related: gateway_security_patterns.md
- Related: codex_integration_patterns.md
