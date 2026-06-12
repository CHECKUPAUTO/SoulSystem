# Lazy Loading Pattern

**Source:** OpenEvolve Night Cycle 2026-04-12 23:33  
**Priority:** Medium  
**Impact:** Startup performance, code consistency

## Problem

The barrel-elimination campaign has reduced circular dependencies, but modules still load eagerly at startup. This creates cold-start pressure even when functionality isn't immediately needed.

## Pattern: `LazyModule<T>`

A unified wrapper for consistent deferred module loading:

```typescript
export class LazyModule<T> {
  private module: T | null = null;
  private promise: Promise<T> | null = null;

  constructor(private loader: () => Promise<T>) {}

  async get(): Promise<T> {
    if (this.module) return this.module;
    if (!this.promise) this.promise = this.loader();
    this.module = await this.promise;
    return this.module;
  }
}
```

### Usage

```typescript
// Before: eager import (loaded at startup)
import { SessionStore } from './session-store';

// After: lazy import (loaded on first access)
export const sessionStore = new LazyModule(() => import('./session-store'));
export const authProviders = new LazyModule(() => import('./auth-providers'));

// Access pattern
const store = await sessionStore.get();
await store.createSession(...);
```

### Benefits

- **Startup speed:** Modules only load when their functionality is first requested
- **Consistency:** Single pattern for all lazy-loaded modules
- **Composability:** Works with barrel-eliminated direct imports
- **Testability:** LazyModule can be mocked or pre-loaded in tests

### Related References

- `barrel_bypassing_guide.md` — eliminating barrel re-exports
- `performance_optimization_patterns.md` — static lookup optimization
- `startup_context_extraction_pattern.md` — session state preloading