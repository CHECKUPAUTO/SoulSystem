# Session State Management Patterns

**Based on OpenEvolve Night Cycle Analysis**  
**Generated:** 2026-04-11  
**Source Reports:** night_cycle_20260411_0630.md, night_cycle_20260411_0633.md

---

## Pattern Overview

The OpenClaw codebase has been undergoing systematic **runtime state extraction** refactoring to improve testability and reduce import cycles.

### State Module Pattern

```typescript
// STATE MODULE PATTERN
// src/feature/feature-state.ts - Pure state + selectors
export interface FeatureState { ... }
export const selectFeature = (state: FeatureState) => ...;

// src/feature/feature.ts - Business logic
import { FeatureState } from './feature-state';
```

### Example Implementations

Recent commits have extracted state modules:

| Module | State File | Lines |
|--------|-----------|-------|
| Context | `context-runtime-state.ts` | 37 |
| Models Config | `models-config-state.ts` | 29 |
| Store Lock | `store-lock-state.ts` | 51 |
| Media | `media/store.runtime.ts` | varies |

---

## Benefits

1. **Better Testability** - Mockable state modules
2. **Reduced Import Cycles** - Clear dependency boundaries
3. **Clearer Separation of Concerns** - State separated from logic
4. **Runtime State Management** - Easier to track and reset

---

## Migration Checklist

- [ ] Identify modules with mixed state and logic
- [ ] Create `*-state.ts` module with pure state + selectors
- [ ] Update imports in dependent modules
- [ ] Add runtime state testing utilities

---

## Testing Pattern

```typescript
// With extracted state
export function withRuntimeState<T>(
  state: RuntimeState,
  fn: () => T
): T {
  const prev = getCurrentState();
  setRuntimeState(state);
  try { return fn(); } finally { setRuntimeState(prev); }
}
```

---

## References

- Commits: `a898cd4` through `ba7297f`
- Related Pattern: Plugin Barrel Avoidance (Pattern 1)
- Risk Level: Low (refactoring with existing tests)
