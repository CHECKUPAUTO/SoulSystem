# Pattern: Runtime State Extraction (Startup Context)

**Classification:** Architecture Pattern | **Safety Level:** Documentation Only | **Source:** night_cycle_20260412_0301.md

## Overview

The Runtime State Extraction pattern separates runtime state management from business logic to improve testability, reduce import cycles, and create clearer separation of concerns.

## Pattern Structure

```typescript
// STATE MODULE PATTERN
// src/feature/feature-state.ts - Pure state + selectors
export interface FeatureState { 
  // State properties
}

export const selectFeature = (state: FeatureState) => ...;
```

## Implementation Examples

### Startup Context (Commit 4d0f5553)

```typescript
// src/auto-reply/reply/startup-context.ts
export interface StartupContext {
  memory: MemoryEntry[];
  templates: AgentTemplate[];
  runtime: RuntimeConfig;
}

export async function preloadStartupContext(): Promise<StartupContext> {
  // Preloads startup memory for bare session resets
  // Implementation handles async loading and validation
}
```

### State Module Files

Recent implementations in OpenClaw:

| File | Lines | Purpose |
|------|-------|---------|
| `context-runtime-state.ts` | 37 | Context engine runtime state |
| `models-config-state.ts` | 29 | Model configuration state |
| `store-lock-state.ts` | 51 | Store locking mechanism state |
| `media/store.runtime.ts` | - | Media storage runtime state |

## Benefits

1. **Better Testability** - State can be mocked and tested independently
2. **Reduced Import Cycles** - State modules have minimal dependencies
3. **Clearer Separation** - Business logic vs state management separated
4. **Reusability** - State selectors can be reused across components

## CodeWiki Entry

**Pattern ID:** `patterns/startup-context-extraction`  
**Related Patterns:** 
- `runtime-registration-pattern`
- `barrel-bypass-pattern`
- `test-seams-pattern`

## Implementation Guidelines

### DO:
- Extract state interfaces to `*-state.ts` files
- Keep state modules free of side effects
- Use pure functions for state selectors
- Document state shape with JSDoc

### DON'T:
- Mix business logic in state files
- Create circular dependencies between state modules
- Import heavy dependencies in state files

## Related Commits

- `4d0f5553` - Preload startup memory for bare session resets
- `c31aa6da` - Preserve parent channel context for recall runs
- `6800579e` - Active memory fallback cleanup

## References

- Session State Management: `session_state_management_patterns.md`
- Cross-Project Analysis: `cross_project_ecosystem_analysis_2026-04-11.md`
