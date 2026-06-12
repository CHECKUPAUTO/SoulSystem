# Subagent Lifecycle State Machine

**Priority:** P1 (High)
**Source:** Night Cycle 2026-04-13 04:31, 04:05 (commits 65469, 65456, 65478, 65597)
**Status:** Reference documentation
**Applies to:** OpenClaw agents/subagents, session management

---

## Problem

Multiple race conditions in subagent lifecycle suggest implicit state management:
- Session lock exit listener leaks (#65469)
- Kill-recovery hook bootstrap race
- Duplicate subagent ended hook loads
- Active-turn queued user prompts not preserved (#65478)
- GPT-5 prompt/retry split reveals mutating action ambiguity (#65597)

These indicate the subagent lifecycle lacks a formal state machine with transition guards.

## Proposed State Machine

```
                    ┌──────────────────────────────────┐
                    │                                  │
                    ▼                                  │
  [Created] ──→ [Initializing] ──→ [Running] ──→ [Yielding]
                    │                   │                  │
                    │                   │                  │
                    ▼                   ▼                  ▼
               [Failed]           [Completed]        [Resuming]
                    │                   │                  │
                    │                   │                  │
                    ▼                   ▼                  │
              [Terminated]        [Ended] ◄────────────────┘
                                        │
                                        ▼
                                   [CleanedUp]
```

### State Definitions

| State | Description | Allowed Transitions |
|-------|-------------|---------------------|
| Created | Subagent spawned, no resources allocated | → Initializing |
| Initializing | Loading context, setting up session, connecting | → Running, Failed |
| Running | Actively processing, tool calls flowing | → Yielding, Completed, Failed |
| Yielding | Returning intermediate result, waiting for parent | → Resuming, Terminated |
| Resuming | Continuing after yield, restoring context | → Running, Failed |
| Completed | Successfully finished, result available | → Ended |
| Failed | Error occurred, may retry | → Terminated, Initializing (retry) |
| Terminated | Killed externally or after max retries | → CleanedUp |
| Ended | Final result delivered to parent | → CleanedUp |
| CleanedUp | All resources released, hooks unregistered | (terminal) |

### Transition Guards

```typescript
interface TransitionGuard {
  from: SubagentState;
  to: SubagentState;
  precondition: () => boolean;
  action: () => Promise<void>;
}

// Example: Yielding → Resuming requires active parent session
const yieldToResume: TransitionGuard = {
  from: 'Yielding',
  to: 'Resuming',
  precondition: () => parentSession?.isActive ?? false,
  action: async () => { await restoreContext(yieldedState); }
};

// Example: Running → Completed requires result available
const runningToEnd: TransitionGuard = {
  from: 'Running',
  to: 'Completed',
  precondition: () => result !== undefined,
  action: async () => { await cleanupResources(); }
};
```

### Invariant Checks

1. **No duplicate hook loads** — Track registered hooks per state transition
2. **No leaked exit listeners** — CleanUp state must unregister all listeners
3. **Mutating actions tracked** — Running state knows which actions are mutating
4. **Queued prompts preserved** — Yielding state must preserve queued user prompts

## Fixes That Would Have Been Caught

| Bug | Root Cause | State Machine Guard |
|-----|-----------|-------------------|
| Session lock leak (#65469) | No cleanup on termination | CleanUp → unregister all listeners |
| Kill-recovery race | No transition guard on kill | Terminating guard blocks bootstrap |
| Duplicate ended hooks | No hook registration tracking | Track hooks per transition |
| Lost queued prompts (#65478) | Yield drops queue | Yielding → preserve queue in yieldedState |
| Mutating retry ambiguity (#65597) | No action classification | Running → classify mutating vs idempotent |

## Related References

- `mutating_action_registry.md` — Mutating vs idempotent action classification
- `service_lifecycle_pattern.md` — Two-phase startup with dependency declarations
- `incomplete_turn_handling.md` — GPT-5.4 runtime completion rollup
- `deferred_activation_pattern.md` — Deferred activation for startup race conditions