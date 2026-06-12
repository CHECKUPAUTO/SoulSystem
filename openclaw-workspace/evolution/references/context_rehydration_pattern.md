# Context Rehydration Pattern
**Generated:** 2026-04-11 23:53 UTC
**Source:** night_cycle_20260411_2346.md (commit 8fabfa5)
**Status:** ✅ Documented Best Practice

---

## Overview

When modules are reloaded (e.g., during evolution, hot-patching, or HMR), runtime state must be explicitly re-synced from the runtime store. This document describes the context token cache rehydration pattern.

---

## The Problem

During module reload:
1. In-memory caches are cleared
2. New module instance has empty state
3. Tokens/contexts from previous instance are lost
4. Subsequent requests fail with "cache miss" or stale data

---

## The Solution

Commit `8fabfa5d1c` - "fix: rehydrate context token cache after module reload"

```typescript
// After module reload, restore from runtime store
export function rehydrateContextCache(): void {
  const storedTokens = runtimeStore.get('context.tokens');
  if (storedTokens) {
    tokenCache.populate(storedTokens);
  }
}

// Call during module initialization
if (module.hot?.data) {
  rehydrateContextCache();
}
```

---

## Implementation Pattern

### 1. Extract State Before Reload

```typescript
if (import.meta.hot) {
  import.meta.hot.dispose((data) => {
    data.tokens = tokenCache.serialize();
  });
}
```

### 2. Rehydrate on Load

```typescript
if (import.meta.hot?.data) {
  tokenCache.populate(import.meta.hot.data.tokens);
}
```

### 3. Fallback to Persistent Store

```typescript
export function rehydrateContextCache(): void {
  // Try hot reload data first
  if (import.meta.hot?.data?.tokens) {
    tokenCache.populate(import.meta.hot.data.tokens);
    return;
  }
  
  // Fall back to runtime store
  const storedTokens = runtimeStore.get('context.tokens');
  if (storedTokens) {
    tokenCache.populate(storedTokens);
  }
}
```

---

## Security Considerations

When rehydrating caches:

1. **Validate restored data** - Don't trust serialized state blindly
2. **Check expiry** - Tokens may have expired during reload
3. **Mutation guards** - Apply security guards after rehydration (see `security_audit_patterns.md`)

```typescript
function rehydrateSecurely(storedData: unknown): void {
  if (!isValidTokenCache(storedData)) {
    logger.warn('Invalid token cache data, skipping rehydration');
    return;
  }
  
  tokenCache.populate(storedData);
  
  // Re-apply mutation guards
  applyMutationGuards();
}
```

---

## Usage with OpenClaw Evolution

When OpenEvolve hot-patches modules:

1. System detects module change
2. Current state is serialized to runtime store
3. New module instance loaded
4. Rehydration function called automatically
5. State is restored, execution continues

---

## Related Patterns

- `session_state_management_patterns.md` - Runtime state extraction
- `security_audit_patterns.md` - Mutation guards

---

*Auto-generated from OpenEvolve Night Cycle analysis*
