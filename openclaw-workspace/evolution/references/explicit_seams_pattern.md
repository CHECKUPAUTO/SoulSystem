# Explicit Seams Pattern

**Source:** OpenEvolve Night Cycle Report 2026-04-12 03:30 UTC  
**Priority:** P0 - Critical Architecture  
**Use Case:** Eliminate circular dependencies through explicit module boundaries

---

## Problem Statement

Barrel files (`index.ts` aggregating exports) create implicit coupling graphs that:
- Create circular import dependencies
- Degrade build times and bundle sizes
- Reduce developer velocity
- Lower fitness scores (syntax: -15%, semantic: -10%)

**T430 Assessment:** System reached complexity threshold (38K+ nodes) requiring structural mutation.

---

## Solution: Explicit Seams

Replace barrel files with explicit per-module imports:

### BEFORE (Barrel Pattern - High Coupling)

```typescript
// index.ts (barrel file)
export { Runtime } from './runtime/Runtime';
export { Config } from './config/Config';
export { Utils } from './utils/Utils';
export { Session } from './session/Session';

// Consumer imports everything through barrel
import { Runtime, Config, Utils, Session } from '@openclaw/core';
```

**Problems:**
- Consumer imports entire module graph even if only using one export
- Circular dependencies hide in the barrel aggregation
- Changes to any export trigger downstream rebuilds

### AFTER (Explicit Seams - Low Coupling)

```typescript
// NO barrel file - consumers import directly

// Consumer imports only what they need
import { Runtime } from '@openclaw/core/runtime/Runtime';
import { Config } from '@openclaw/core/config/Config';
import { Utils } from '@openclaw/core/utils/Utils';

// Alternative: Use path mapping for cleaner imports
import { Runtime } from '@openclaw/core/runtime';
import { Config } from '@openclaw/core/config';
```

**Benefits:**
- Tree-shaking works correctly
- Dependencies are explicit
- Changes isolated to actual consumers
- Circular dependencies become visible

---

## Implementation Strategy

### Phase 1: Identify Barrel Files

```bash
# Find all barrel index.ts files
find src -name "index.ts" -type f | while read f; do
  # Check if file only contains exports
  if grep -q "^export" "$f" && ! grep -q "^import" "$f"; then
    echo "Barrel candidate: $f"
  fi
done
```

### Phase 2: Inline Imports (Automated)

```typescript
// BEFORE: Import from barrel
import { Runtime, Config } from '@openclaw/core';

// AFTER: Inline to direct import
import { Runtime } from '@openclaw/core/runtime/Runtime';
import { Config } from '@openclaw/core/config/Config';
```

### Phase 3: Remove Barrel Files

Once all imports are inlined, remove the barrel `index.ts` files.

### Phase 4: Update Path Mappings (Optional)

```json
// tsconfig.json
{
  "compilerOptions": {
    "paths": {
      "@openclaw/core/runtime": ["./src/runtime"],
      "@openclaw/core/config": ["./src/config"],
      // NOT: "@openclaw/core": ["./src"] - no barrel
    }
  }
}
```

---

## Rules

1. **No barrel files at package boundaries**
   - Each module exports only its own types
   - No aggregation of child module exports

2. **Type imports separate from value imports**
   ```typescript
   import type { Config } from '@openclaw/core/config/Config';
   import { loadConfig } from '@openclaw/core/config/loadConfig';
   ```

3. **Internal modules use `*_internal.ts` suffix**
   ```typescript
   // Public API
   import { Runtime } from '@openclaw/core/runtime/Runtime';
   
   // Internal implementation
   import { runtimeInternals } from '@openclaw/core/runtime/runtime_internal';
   ```

4. **Circular dependency detection in CI**
   ```bash
   # Using madge
   npx madge --circular src/
   
   # Fail build if circular dependencies found
   if [ $? -ne 0 ]; then
     echo "Circular dependencies detected!"
     exit 1
   fi
   ```

---

## Pattern Recognition

### Type Seam Splitting

Split monolithic type files into focused modules:

```typescript
// BEFORE: types.ts (monolithic)
export interface Session { ... }
export interface SessionHook { ... }
export interface SessionEvent { ... }
export interface SessionPayload { ... }

// AFTER: Split by concern
// src/session/types/Session.ts
export interface Session { ... }

// src/session/types/SessionHook.ts
export interface SessionHook { ... }

// src/session/types/SessionEvent.ts
export interface SessionEvent { ... }

// src/session/types/SessionPayload.ts
export interface SessionPayload { ... }
```

### Narrow Import Surfacing

Export only what callers need:

```typescript
// BEFORE: Broad export
export * from './runtime';

// AFTER: Narrow exports
export { Runtime } from './runtime/Runtime';
export type { RuntimeConfig } from './runtime/RuntimeConfig';
// NOT exporting: RuntimeInternals, RuntimeHelpers, etc.
```

---

## Expected Fitness Gain

| Metric | Before | After | Change |
|--------|--------|-------|--------|
| syntax_score | 58/100 | 78/100 | +20% |
| semantic_score | 55/100 | 70/100 | +15% |
| build_time | baseline | -30% | faster |
| bundle_size | baseline | -25% | smaller |

---

## Migration Guide

### Commit Message Pattern

```
fix(cycles): bypass [module] barrels

- Replace barrel imports with direct module imports
- Narrow [specific type] surface
- Part of barrel elimination campaign
```

### Example Commits from Night Cycle

- `fix(cycles): bypass context engine and config barrels`
- `fix(cycles): bypass store and channel barrels`
- `fix(cycles): narrow channel registry imports`
- `fix(cycles): split session hook event types`
- `fix(cycles): split reply payload and option contracts`

---

## References

- Night Cycle Report: `night_cycle_20260412_0330.md`
- IronReview T430: `ironreview_t430_integration.md`
- Madge (circular dependency detector): https://github.com/pahen/madge

---

*Generated by OpenEvolve Night Cycle*  
*Classification: P0 Architecture Pattern*
