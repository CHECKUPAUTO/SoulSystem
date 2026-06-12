# Session Context Propagation Rules

**Source:** OpenEvolve Night Cycle Analysis (2026-04-11)  
**Scope:** OpenClaw Session/Target Entry Resolution

---

## Problem Statement

Previous code was using requester/session context instead of target context, causing data to be written to wrong locations. This led to:
- Session key collisions
- Metadata written to wrong entries
- Binding context mismatches
- Agent directory confusion

## The Fix Pattern

### Before (Bug)

```typescript
// WRONG: Using requester context
async function processToolCall(request: ToolRequest) {
  const session = request.session; // Requester session
  const agentDir = session.agentDir; // Wrong directory!
  
  await executeTool(tool, { agentDir });
}
```

### After (Fixed)

```typescript
// CORRECT: Using target context
async function processToolCall(request: ToolRequest) {
  const targetEntry = request.targetEntry ?? request.session;
  const agentDir = targetEntry.agentDir; // Correct directory!
  
  await executeTool(tool, { agentDir });
}
```

---

## Target Entry Resolution Rules

### Priority Order

When determining which context to use, follow this priority:

1. **Target Entry** (`targetEntry`) - Explicit target if available
2. **Target Session** (`targetSession`) - For cross-session operations
3. **Requester Session** (`session`) - Fallback for own-session operations
4. **Main Session** - For global operations

### Decision Tree

```
Is there an explicit target?
├── Yes → Use target entry
│         └── Is target a session?
│               ├── Yes → Use target session context
│               └── No → Use target's agent directory
└── No → Use requester context
          └── Is requester a subagent?
                ├── Yes → Use parent session
                └── No → Use requester session
```

---

## Affected Areas

### 1. Entry/Metadata Operations (25 fixes)

| Operation | Fix |
|-----------|-----|
| Inline abort | Prefer target entry |
| Usage footer | Prefer target entry |
| Reply directives | Prefer target entry |
| Binding metadata | Prefer target entry |

### 2. Agent Directory Resolution (20 fixes)

| Scenario | Correct Directory |
|----------|-------------------|
| `btw` command | Target agent dir |
| `compact` | Target agent dir |
| Directive persistence | Target agent dir |
| Tool execution | Target agent dir |
| Session export | Target agent dir |

### 3. Session Context Operations (18 fixes)

| Operation | Context |
|-----------|---------|
| `bash` tool | Target session |
| Models selection | Target session |
| Export | Target session |
| Abort | Target session |

---

## Implementation Pattern

### Type-Safe Resolution

```typescript
interface ContextResolution {
  // Primary target context
  targetEntry: Session | Agent | undefined;
  
  // Derived contexts
  targetSession: Session | undefined;
  targetAgent: Agent | undefined;
  
  // Requester context (fallback only)
  requesterSession: Session;
}

function resolveContext(request: Request): ContextResolution {
  const targetEntry = request.targetEntry;
  
  return {
    targetEntry,
    targetSession: targetEntry?.type === 'session' ? targetEntry : undefined,
    targetAgent: targetEntry?.type === 'agent' ? targetEntry : undefined,
    requesterSession: request.session,
  };
}

function getEffectiveSession(resolution: ContextResolution): Session {
  // Priority: target session > requester session
  return resolution.targetSession ?? resolution.requesterSession;
}

function getAgentDir(resolution: ContextResolution): string {
  // Priority: target agent dir > requester agent dir
  const session = getEffectiveSession(resolution);
  return resolution.targetAgent?.agentDir ?? session.agentDir;
}
```

---

## Key Collision Prevention

### Anti-Pattern Identified

Using request-scoped identifiers as persistence keys without namespacing causes collisions.

### Solution: Hierarchical Keys

```typescript
// BEFORE (prone to collisions)
const key = `${sessionId}-${agentId}`;

// AFTER (namespaced)
const key = `${agentId}/sessions/${sessionId}/bindings/${bindingType}`;

// Or use structured key objects
interface BindingKey {
  agent: string;
  session: string;
  type: BindingType;
}

function serializeKey(key: BindingKey): string {
  return `${key.agent}/sessions/${key.session}/${key.type}`;
}
```

### Collision-Prone Areas Fixed

| Area | Pattern | Fix |
|------|---------|-----|
| qqbot session files | Flat key space | Namespaced with prefix |
| Teams SSO tokens | Flat key space | Namespaced with provider |
| Device-pair subscribers | Unscoped IDs | Namespaced with device ID |
| Subagent registry | Registry key collision | Normalized session keys |

---

## Binding Metadata Preservation

### Problem: Rebinding Loses Context

When sessions rebind, metadata was being reset/narrowed instead of preserved.

### Solution: Context Restore

```typescript
interface BindingContext {
  conversationId: string;
  lifecycleWindows: LifecycleWindow[];
  focusedBinding: Binding | null;
  metadata: Record<string, unknown>;
}

async function rebindSession(
  session: Session,
  newBinding: Binding,
  previousContext?: BindingContext
): Promise<void> {
  // Preserve previous context
  const contextToRestore = previousContext ?? {
    conversationId: session.conversationId,
    lifecycleWindows: session.lifecycleWindows,
    focusedBinding: session.focusedBinding,
    metadata: session.metadata,
  };
  
  // Apply to new binding
  await applyBindingContext(newBinding, contextToRestore);
}
```

---

## Testing Guidelines

### Context Propagation Test

```typescript
describe('context propagation', () => {
  it('should use target entry for tool execution', async () => {
    const requesterSession = createSession({ agentDir: '/requester' });
    const targetSession = createSession({ agentDir: '/target' });
    
    const request = createRequest({
      session: requesterSession,
      targetEntry: targetSession,
      tool: 'bash',
    });
    
    await processToolCall(request);
    
    // Assert: Uses target's agent directory
    expect(executeTool).toHaveBeenCalledWith(
      expect.anything(),
      expect.objectContaining({ agentDir: '/target' })
    );
  });
  
  it('should fallback to requester when no target', async () => {
    const requesterSession = createSession({ agentDir: '/requester' });
    
    const request = createRequest({
      session: requesterSession,
      targetEntry: undefined,
      tool: 'bash',
    });
    
    await processToolCall(request);
    
    // Assert: Falls back to requester
    expect(executeTool).toHaveBeenCalledWith(
      expect.anything(),
      expect.objectContaining({ agentDir: '/requester' })
    );
  });
});
```

---

## Common Mistakes

### 1. Direct Session Access

```typescript
// WRONG
const dir = request.session.agentDir;

// CORRECT
const dir = request.targetEntry?.agentDir ?? request.session.agentDir;
```

### 2. Ignoring Target Session

```typescript
// WRONG
const session = request.session;
const model = session.models.selected;

// CORRECT
const effectiveSession = request.targetSession ?? request.session;
const model = effectiveSession.models.selected;
```

### 3. Flat Key Spaces

```typescript
// WRONG
const key = `${sessionId}-binding`;

// CORRECT
const key = `${agentId}/sessions/${sessionId}/bindings`;
```

---

## Migration Checklist

When updating existing code:

- [ ] Identify all `request.session` accesses
- [ ] Determine if target context should be preferred
- [ ] Update to use `targetEntry ?? session` pattern
- [ ] Add test cases for target vs requester contexts
- [ ] Check for key collision vulnerabilities
- [ ] Add runtime assertions for duplicate keys

---

## References

- Session key collision fixes (15+ commits)
- Binding context restore fixes (8 commits)
- Agent directory resolution fixes (12 commits)
- Model fallback persistence fix (commit 3b139862)

---

*Generated by OpenEvolve Auto-Apply*  
*Timestamp: 2026-04-11T04:27:00Z*
