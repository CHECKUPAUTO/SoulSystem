# Context Token Cache Rehydration Pattern

**Source:** OpenEvolve Night Cycle Analysis (2026-04-11)  
**Scope:** OpenClaw Context Window Runtime State Management

---

## Problem Statement

When `CONTEXT_WINDOW_RUNTIME_STATE.configuredConfig` was already populated, the function returned early WITHOUT re-applying the cached configuration to the `MODEL_CONTEXT_TOKEN_CACHE`. This caused module reloads to retain stale state, resulting in token calculation mismatches.

## The Fix (Commit 8fabfa5)

### Before (Bug)

```typescript
function getConfiguredContextWindows(): Config | undefined {
  if (CONTEXT_WINDOW_RUNTIME_STATE.configuredConfig) {
    // BUG: Returns cached config but doesn't rehydrate token cache
    return CONTEXT_WINDOW_RUNTIME_STATE.configuredConfig;
  }
  
  // ... load and apply configuration
}
```

### After (Fixed)

```typescript
function getConfiguredContextWindows(): Config | undefined {
  if (CONTEXT_WINDOW_RUNTIME_STATE.configuredConfig) {
    // FIX: Explicitly rehydrate the token cache
    applyConfiguredContextWindows({
      cache: MODEL_CONTEXT_TOKEN_CACHE,
      modelsConfig: CONTEXT_WINDOW_RUNTIME_STATE.configuredConfig.models as
        | ModelsConfig
        | undefined,
    });
    return CONTEXT_WINDOW_RUNTIME_STATE.configuredConfig;
  }
  
  // ... load and apply configuration
}
```

---

## Pattern: Runtime State Rehydration

### When to Apply

This pattern applies when:
1. Runtime state is cached for performance
2. Module reloads or hot-reloads occur
3. Derived state (calculations, aggregations) depends on the cached state
4. Multiple subsystems reference the same configuration

### Implementation Checklist

- [ ] Identify all runtime caches that depend on configuration
- [ ] Add explicit rehydration calls after config restoration
- [ ] Ensure idempotency (rehydration can run multiple times safely)
- [ ] Add tests for module reload scenarios
- [ ] Document the dependency chain

---

## Media Subsystem: Runtime Separation Pattern

### Architecture

The media subsystem was refactored to separate runtime concerns:

```
BEFORE:
  server.ts → imports from ../infra/fs-safe.js
  store.ts  → imports from ../infra/fs-safe.js

AFTER:
  server.ts     → imports from ./server.runtime.js
  store.ts      → imports from ./store.runtime.js
  server.runtime.ts  → re-exports with runtime-safe wrappers
  store.runtime.ts   → re-exports with runtime-safe wrappers
```

### Benefits

1. **Testability**: Mock runtime state without touching fs-safe internals
2. **Bundle Size**: Runtime bundles exclude infra/fs-safe when not needed
3. **Type Safety**: `isSafeOpenError()` type guard replaces `instanceof` checks

---

## Tech Debt Items Identified

### 1. Inconsistent Error Handling

**Issue**: Some places still use `instanceof SafeOpenError`

**Fix**: Migrate to type guards:
```typescript
export function isSafeOpenError(err: unknown): err is SafeOpenError {
  return err instanceof SafeOpenError;
}
```

### 2. Test Setup Coupling

**Issue**: `setup-openclaw-runtime.ts` has cross-module dependencies

**Fix**: Continue slimming, consider dependency injection container

### 3. Missing Test Coverage

**Issue**: New `context.lookup.test.ts` only has basic tests

**Fix**: Add negative test cases for failed cache rehydration

---

## Migration Guide

### For Module Authors

When extracting runtime state:

1. Create `{module}.runtime.ts` alongside `{module}.ts`
2. Move runtime-only imports to `.runtime.ts`
3. Re-export through runtime adapter
4. Update imports in source files
5. Add test helpers for mocking runtime state

### Example

```typescript
// server.runtime.ts
import { safeOpen, SafeOpenError } from '../infra/fs-safe.js';

export { safeOpen, SafeOpenError };

export function isSafeOpenError(err: unknown): err is SafeOpenError {
  return err instanceof SafeOpenError;
}

// server.ts
import { safeOpen, isSafeOpenError } from './server.runtime.js';

// Now safe to use with proper type guards
```

---

## Audit Recommendations

### Search for Similar Bugs

```bash
# Pattern to find similar issues
grep -r "if.*RUNTIME_STATE.*configured" src/ --include="*.ts"

# Modules to check:
# - models-config.ts
# - plugins state
# - gateway config cache
# - any other runtime state caches
```

### Cache Consistency Audit

Check for missing rehydration in:
- [ ] models-config.ts
- [ ] plugins state
- [ ] gateway config cache
- [ ] session store
- [ ] agent context

---

## Testing Patterns

### Module Reload Test

```typescript
describe('context window cache', () => {
  it('should rehydrate token cache after module reload', async () => {
    // Arrange: Configure context windows
    const config = await configureContextWindows({
      models: { 'gpt-4': { contextWindow: 8192 } }
    });
    
    // Act: Simulate module reload
    clearModuleCache();
    const reloaded = await getConfiguredContextWindows();
    
    // Assert: Token cache is restored
    expect(MODEL_CONTEXT_TOKEN_CACHE.get('gpt-4')).toBe(8192);
    expect(reloaded).toEqual(config);
  });
});
```

---

## References

- Commit 8fabfa5: Context token cache rehydration
- Commit ba7297f: Media fs-safe test seams
- Commit a764b8f: Test setup agent imports

---

*Generated by OpenEvolve Auto-Apply*  
*Timestamp: 2026-04-11T04:27:00Z*
