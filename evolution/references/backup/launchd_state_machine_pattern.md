# LaunchD State Machine Pattern
**Source:** OpenEvolve Night Cycle Analysis (night_cycle_20250410_2347.md)
**Created:** 2026-04-11 by OpenEvolve Auto-Apply

## Problem

LaunchD integration required 5+ fixes in rapid succession, indicating:
- State machine ambiguity ("running state without pid")
- Ownership model issues ("keep launchd enable scoped to owned stops")
- Handoff fragility ("sanitize launchd handoff label errors")

## Solution: Explicit State Machine

```typescript
type LaunchDState = 'installed' | 'enabled' | 'running' | 'stopped' | 'unloaded';
type Ownership = 'owned' | 'inherited' | 'external';

interface LaunchDServiceState {
  state: LaunchDState;
  ownership: Ownership;
  pid?: number;
  label: string;
  lastTransition: number;
  transitionHistory: StateTransition[];
}

interface StateTransition {
  from: LaunchDState;
  to: LaunchDState;
  trigger: string;
  timestamp: number;
}
```

## State Transitions

```
                    load
         ┌──────────────────────────┐
         │                          │
         ▼                          │
┌──────────────┐    enable    ┌──────────────┐
│   installed  │ ────────────▶ │   enabled    │
└──────────────┘               └──────────────┘
       │                             │
       │ unload                      │ start
       │                             ▼
       │                      ┌──────────────┐
       │                      │    running   │
       │                      └──────────────┘
       │                             │
       │                             │ stop
       │                             ▼
       │                      ┌──────────────┐
       │                      │    stopped   │
       │                      └──────────────┘
       │                             │
       └─────────────────────────────┘
                    disable
```

## Implementation

```typescript
class LaunchDStateMachine {
  private state: LaunchDServiceState;
  private readonly maxHistorySize = 100;

  transition(to: LaunchDState, trigger: string): boolean {
    const current = this.state.state;
    
    if (!this.isValidTransition(current, to)) {
      throw new InvalidTransitionError(current, to);
    }
    
    this.state.transitionHistory.push({
      from: current,
      to: to,
      trigger,
      timestamp: Date.now()
    });
    
    // Prune old history
    if (this.state.transitionHistory.length > this.maxHistorySize) {
      this.state.transitionHistory.shift();
    }
    
    this.state.state = to;
    this.state.lastTransition = Date.now();
    
    return true;
  }

  private isValidTransition(from: LaunchDState, to: LaunchDState): boolean {
    const validTransitions: Record<LaunchDState, LaunchDState[]> = {
      'installed': ['enabled', 'unloaded'],
      'enabled': ['running', 'installed', 'unloaded'],
      'running': ['stopped', 'unloaded'],
      'stopped': ['enabled', 'running', 'unloaded'],
      'unloaded': ['installed']
    };
    
    return validTransitions[from]?.includes(to) ?? false;
  }

  getHistory(): StateTransition[] {
    return [...this.state.transitionHistory];
  }
}
```

## Benefits

1. **Explicit States**: No ambiguity about current state
2. **History Tracking**: Debug complex lifecycle issues
3. **Validation**: Invalid transitions caught at compile/runtime
4. **Testing**: State machine can be unit tested independently

## Testing

```typescript
describe('LaunchDStateMachine', () => {
  it('should handle stop without pid', () => {
    const sm = new LaunchDStateMachine();
    sm.transition('enabled', 'test');
    sm.transition('running', 'test');
    
    // Should allow stop even if pid is undefined
    expect(() => sm.transition('stopped', 'stop-signal')).not.toThrow();
  });

  it('should reject invalid transitions', () => {
    const sm = new LaunchDStateMachine();
    expect(() => sm.transition('running', 'test')).toThrow(InvalidTransitionError);
  });
});
```

## References

- OpenClaw Commits: f3c143f0cd, eebad7a372, c0ddcf6630, 23d9a100c4
- Night Cycle: night_cycle_20250410_2347.md
