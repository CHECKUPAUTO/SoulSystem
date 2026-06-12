# Context Tree Pattern

**Source:** OpenEvolve Night Cycle Report 2026-04-12 03:30 UTC  
**Priority:** P1 - High  
**Use Case:** Isolate session state to prevent context bleed across recall runs

---

## Problem Statement

The active-memory subsystem shows signs of **context bleed** - state pollution across session boundaries:

**Evidence:**
- Parent channel context lost for recall runs (commit c31aa6da)
- Configuration schema fallback field issues (commit 00d0dcf)
- Duplicate commits for fallback model removal (indicating git workflow friction)

**Impact:**
- State pollution across session boundaries
- Unpredictable behavior in recall operations
- Hard-to-debug context propagation issues

---

## Solution: Immutable Context Trees

Replace global state with explicit, immutable context trees:

```
SessionContext (root)
├── ChannelRef (narrow reference)
├── MemorySnapshot
└── ChildContext (recall run)
    ├── ParentRef → SessionContext
    ├── ChannelRef (preserved from parent)
    └── ChildContext (nested recall)
        └── ParentRef → ChildContext
```

---

## Implementation

### Core Context Interface

```typescript
// src/context/SessionContext.ts

import { ChannelRef } from '../channel/ChannelRef';
import { MemorySnapshot } from '../memory/MemorySnapshot';

/**
 * Immutable session context forming a tree structure.
 * Each session operation receives its context explicitly.
 */
export interface SessionContext {
  readonly sessionId: string;
  readonly channel: ChannelRef;        // Narrow reference, not full Channel
  readonly memory: MemorySnapshot;
  readonly parentContext?: SessionContext;  // Tree structure for recalls
  readonly createdAt: Date;
  readonly metadata: Record<string, unknown>;
}

/**
 * Factory for creating root contexts
 */
export function createRootContext(
  sessionId: string,
  channel: ChannelRef,
  memory: MemorySnapshot
): SessionContext {
  return Object.freeze({
    sessionId,
    channel,
    memory,
    parentContext: undefined,
    createdAt: new Date(),
    metadata: {}
  });
}

/**
 * Create a child context for recall operations
 */
export function createRecallContext(
  parent: SessionContext,
  newSessionId: string
): SessionContext {
  return Object.freeze({
    sessionId: newSessionId,
    channel: parent.channel,      // Preserve parent channel
    memory: parent.memory,          // Inherit memory snapshot
    parentContext: parent,          // Link to parent
    createdAt: new Date(),
    metadata: { recall: true, parentSessionId: parent.sessionId }
  });
}
```

### Context-Aware Functions

```typescript
// BEFORE: Implicit global state
async function recallRun(prompt: string): Promise<Response> {
  const session = getCurrentSession(); // Global getter - problematic
  return processPrompt(session, prompt);
}

// AFTER: Explicit context parameter
async function recallRun(
  context: SessionContext,
  prompt: string
): Promise<Response> {
  // Context is explicit and immutable
  return processPrompt(context, prompt);
}
```

### Channel Reference Narrowing

```typescript
// BEFORE: Full channel object in context
interface SessionContext {
  channel: Channel;  // Heavy, includes all channel data
}

// AFTER: Narrow reference only
interface ChannelRef {
  channelId: string;
  channelType: 'telegram' | 'discord' | 'whatsapp';
  // NOT included: full config, history, webhooks, etc.
}

interface SessionContext {
  channel: ChannelRef;  // Lightweight reference
}
```

---

## State Management Rules

1. **All session operations require explicit context argument**
   ```typescript
   // ✅ Good: Context passed explicitly
   async function handleMessage(
     context: SessionContext,
     message: Message
   ): Promise<void>
   
   // ❌ Bad: Implicit context
   async function handleMessage(message: Message): Promise<void>
   ```

2. **Parent contexts passed via readonly reference**
   ```typescript
   // Context tree is immutable
   readonly parentContext?: SessionContext;
   ```

3. **No global `getCurrentSession()` or similar**
   ```typescript
   // ❌ Eliminated
   const session = getCurrentSession();
   
   // ✅ Always explicit
   const response = await processRequest(context, request);
   ```

4. **Contexts have explicit lifecycle (create → use → dispose)**
   ```typescript
   const context = createRootContext(sessionId, channel, memory);
   try {
     await processSession(context);
   } finally {
     await disposeContext(context);
   }
   ```

---

## Active-Memory Integration

### Preserving Channel Context for Recall Runs

```typescript
// extensions/active-memory/index.ts

import { SessionContext, createRecallContext } from '../../src/context/SessionContext';

export async function recallWithContext(
  parentContext: SessionContext,
  recallPrompt: string
): Promise<RecallResult> {
  // Create child context preserving parent channel
  const recallContext = createRecallContext(
    parentContext,
    generateRecallSessionId()
  );
  
  // Channel is automatically preserved from parent
  console.log(`Recalling in channel: ${recallContext.channel.channelId}`);
  
  return await executeRecall(recallContext, recallPrompt);
}
```

### Config Schema Fallback

```typescript
// Config-driven only, no built-in fallbacks
function getActiveMemoryConfig(context: SessionContext): ActiveMemoryConfig {
  const config = context.config.activeMemory;
  
  if (!config) {
    throw new ConfigError('activeMemory config missing');
  }
  
  // No built-in fallback - must be explicitly configured
  return config;
}
```

---

## Testing with Context Trees

```typescript
// test/context/mock-context.ts

export function createMockContext(
  overrides: Partial<SessionContext> = {}
): SessionContext {
  return {
    sessionId: 'test-session-123',
    channel: { channelId: 'test-channel', channelType: 'telegram' },
    memory: createEmptyMemory(),
    parentContext: undefined,
    createdAt: new Date(),
    metadata: {},
    ...overrides
  };
}

// Test recall preserves parent context
it('should preserve parent channel context for recall runs', async () => {
  const parentContext = createMockContext({
    channel: { channelId: 'parent-channel', channelType: 'telegram' }
  });
  
  const recallContext = createRecallContext(parentContext, 'recall-456');
  
  expect(recallContext.channel.channelId).toBe('parent-channel');
  expect(recallContext.parentContext).toBe(parentContext);
});
```

---

## Expected Fitness Gain

| Metric | Before | After | Change |
|--------|--------|-------|--------|
| semantic_score | 55/100 | 70/100 | +15% |
| security_score | 65/100 | 80/100 | +15% |
| debuggability | low | high | improved |
| testability | medium | high | improved |

---

## Migration Path

1. **Audit global state usage**
   ```bash
   grep -r "getCurrentSession\|getGlobalContext" src/
   ```

2. **Create SessionContext interfaces**
   - Define narrow references
   - Mark as readonly

3. **Migrate functions to accept context**
   - Add context as first parameter
   - Remove global getters

4. **Update active-memory extension**
   - Implement recall context creation
   - Add integration tests

---

## References

- Night Cycle Report: `night_cycle_20260412_0330.md`
- Active-Memory Issues: Commits c31aa6da, 00d0dcf, 6800579e
- IronReview T430: `ironreview_t430_integration.md`

---

*Generated by OpenEvolve Night Cycle*  
*Classification: P1 Architecture Pattern*
