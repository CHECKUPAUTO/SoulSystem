# Security Audit Patterns Guide

Generated from OpenEvolve Night Cycle Analysis
Date: 2026-04-11

## Overview

This guide documents security patterns identified during OpenEvolve night cycle analysis of OpenClaw, VisionClaw, and related projects.

## Critical Security Patterns

### 1. Plugin Registry Access Pattern

**Anti-Pattern:** Dynamic plugin registry lookups in hot paths

```typescript
// BEFORE: Expensive dynamic lookup
const plugin = pluginRegistry.get('capability');
```

**Pattern:** Static capability maps with O(1) access

```typescript
// AFTER: Fast static lookup
const STATIC_CHANNEL_CAPS: Record<ChannelType, Capability[]> = {
  telegram: ['inlinebuttons', 'reactions', 'markdown'],
  discord: ['threads', 'embeds', 'slash_commands'],
};
const caps = STATIC_CHANNEL_CAPS[channel];
```

### 2. Error Sanitization at Trust Boundaries

**Pattern:** All errors crossing trust boundaries must be sanitized

```typescript
try {
  await handoff(label);
} catch (e) {
  throw new SanitizedError('handoff_failed', safeMessage(e));
}
```

**Key Locations:**
- System API boundaries (launchd, systemd)
- User-facing error messages
- Plugin error propagation

### 3. Mutation Guard Audit Trail

**Pattern:** Log all blocked mutation attempts

```typescript
interface MutationGuardEvent {
  tool: string;
  mutationType: 'config' | 'state' | 'file';
  sessionId: string;
  timestamp: string;
  allowed: boolean;
  reason?: string;
}
```

**Rationale:** Security patches without audit trails leave gaps in incident response.

### 4. Dangerous Command Detection

**Pattern:** Pattern-based dangerous command detection

```typescript
const DANGEROUS_PATTERNS = [
  'curl.*\|.*bash',     // Pipe to shell
  'wget.*\|.*sh',
  'base64.*-d.*\|',    // Decode and pipe
  'eval\(',             // Eval usage
  'python.*-c.*exec',
];
```

### 5. Service State Validation

**Pattern:** Multi-signal state validation (don't rely on single signal)

```typescript
// Don't rely solely on PID
if (isRunning && !pid) {
  return { state: 'running', confidence: 'low' };
}
```

## Trust Boundaries

1. **Plugin Registry** ↔ **Core Gateway**
2. **System APIs** ↔ **Agent Execution**
3. **User Input** ↔ **Command Execution**
4. **Session Memory** ↔ **External Storage**

## Configuration Security

### Timeout Alignment

**Issue:** Multiple timeout defaults diverging

**Solution:** Centralize in dedicated config file

```typescript
// src/config/agent-timeout-defaults.ts
export const DEFAULT_LLM_IDLE_TIMEOUT_SECONDS = 120;

// Propagate through schemas
// types.agent-defaults.ts
// zod-schema.agent-defaults.ts
```

### Static Config vs Dynamic Registry

**Principle:** Prefer static configurations for:
- Channel capabilities
- Timeout values
- Known constant mappings

**Dynamic registry reserved for:**
- User-installed plugins
- Runtime-discovered capabilities
- Environment-specific configuration

## Testing Security

### Hermetic Test Isolation

```typescript
// Good: Explicit mocking
const mockChannel = createMockChannel({
  capabilities: ['reactions'],
  allowFromMode: 'topOrNested'
});

// Bad: Implicit dependency on real channel
const plugin = pluginRegistry.get('discord');
```

### CI Gate Requirements

1. Type check pass
2. Compile check pass
3. Security pattern lint pass
4. Cannot be overridden without admin approval

## Audit Checklist

- [ ] Plugin lookups in hot paths are static
- [ ] Error messages don't leak internal paths
- [ ] Mutation attempts are logged
- [ ] Dangerous commands are detected
- [ ] Service states validated via multiple signals
- [ ] Config defaults are centralized
- [ ] Tests use explicit mocks

## References

- OpenClaw Security Model: VISION.md, SECURITY.md
- Commit e2d93fb: perf: short-circuit static doctor channel capabilities
- Commit cd7168a: fix(gateway): tighten remote mutation guards
- Commit 279cbfc: fix: restore memory wiki and dreaming checks

## Updates

This document evolves with OpenEvolve night cycles. Last updated: 2026-04-11
