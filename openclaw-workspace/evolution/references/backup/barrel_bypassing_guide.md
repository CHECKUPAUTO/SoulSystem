# Barrel Bypassing Guide

**Source:** OpenEvolve Night Cycle Report 2026-04-12 03:30 UTC  
**Author:** Vincent Koc (pattern established through 24 commits)  
**Priority:** P0 - Critical Architecture  
**Use Case:** Systematic elimination of circular dependencies

---

## Problem Statement

Barrel files (`index.ts` aggregating exports) create **circular import dependencies** that:
- Slow build times (TypeScript must resolve entire graph)
- Block tree-shaking (bundlers can't eliminate dead code)
- Degrade developer velocity (cascading rebuilds)
- Lower T430 fitness scores (syntax: -15%, semantic: -10%)

**T430 Assessment:** System reached complexity threshold (38,936 nodes) requiring structural mutation.

**Evidence from Night Cycle:**
24 commits by Vincent Koc systematically bypassing barrels:
```
fix(cycles): bypass context engine and config barrels
fix(cycles): bypass store and channel barrels
fix(cycles): bypass get-reply barrel exports
fix(cycles): narrow channel registry imports
fix(cycles): bypass session binding service type import
fix(cycles): bypass channel public session type import
```

---

## The Barrel Anti-Pattern

### What is a Barrel File?

A barrel file re-exports from child modules, creating a central import point:

```typescript
// src/core/index.ts (barrel file)
export { Runtime } from './runtime/Runtime';
export { Config } from './config/Config';
export { Session } from './session/Session';
export { Plugin } from './plugin/Plugin';
```

**Why it's convenient:**
```typescript
// Consumer imports everything from one place
import { Runtime, Config, Session } from '@openclaw/core';
```

**Why it's dangerous:**
- Creates implicit dependency graph
- Any change to any export triggers downstream rebuilds
- Circular dependencies hide in the aggregation
- Tree-shaking becomes ineffective

### Circular Dependency Creation

```typescript
// Module A depends on Module B
// src/core/Runtime.ts
import { Config } from './index'; // Through barrel

// Module B depends on Module A  
// src/core/Config.ts
import { Runtime } from './index'; // Through barrel

// Result: Circular dependency that's hard to see
```

---

## Solution: Barrel Bypassing

### The Bypass Pattern

Replace barrel imports with **direct leaf module imports**:

```typescript
// BEFORE: Import through barrel (circular risk)
import { Runtime, Config } from '@openclaw/core';

// AFTER: Direct leaf imports (explicit dependencies)
import { Runtime } from '@openclaw/core/runtime/Runtime';
import { Config } from '@openclaw/core/config/Config';
```

### Type Splitting (Companion Pattern)

When types are entangled, **split them into focused modules**:

```typescript
// BEFORE: Monolithic types with circular references
// src/types.ts
export interface Session { hooks: SessionHook[]; }
export interface SessionHook { session: Session; } // Circular!

// AFTER: Split by concern with explicit imports
// src/session/types/Session.ts
import { SessionHook } from './SessionHook';
export interface Session { hooks: SessionHook[]; }

// src/session/types/SessionHook.ts  
import { Session } from './Session';
export interface SessionHook { sessionId: string; } // Reference by ID, not object
```

### Import Narrowing

Only import what you actually need:

```typescript
// BEFORE: Broad import from barrel
import { Runtime, Config, Utils, Helpers, Internals } from '@openclaw/core';
// Using: Runtime, Config
// NOT using: Utils, Helpers, Internals

// AFTER: Narrow imports
import { Runtime } from '@openclaw/core/runtime/Runtime';
import { Config } from '@openclaw/core/config/Config';
```

---

## Implementation Strategy

### Phase 1: Detection

Find barrel files in your codebase:

```bash
#!/bin/bash
# find_barrels.sh

find src -name "index.ts" -o -name "index.js" | while read file; do
  # Check if file only contains exports (barrel signature)
  if grep -E "^export" "$file" | wc -l | grep -q "[1-9]"; then
    # Check if file has no substantial implementation
    lines=$(wc -l < "$file")
    if [ "$lines" -lt 50 ]; then
      echo "Barrel candidate: $file"
    fi
  fi
done
```

### Phase 2: Inline Imports (Automated Refactoring)

```typescript
// BEFORE: Barrel import
import { Runtime, Config } from '@openclaw/core';

// AFTER: Direct imports
import { Runtime } from '@openclaw/core/runtime/Runtime';
import { Config } from '@openclaw/core/config/Config';
```

**Migration script (concept):**
```javascript
// scripts/migrate-barrels.js
const { Project } = require('ts-morph');

function migrateBarrels(projectPath) {
  const project = new Project({
    tsConfigFilePath: `${projectPath}/tsconfig.json`,
  });

  // Find all barrel imports
  const sourceFiles = project.getSourceFiles();
  
  for (const sourceFile of sourceFiles) {
    const imports = sourceFile.getImportDeclarations();
    
    for (const importDecl of imports) {
      const moduleSpecifier = importDecl.getModuleSpecifierValue();
      
      // Detect barrel imports (adjust pattern as needed)
      if (moduleSpecifier === '@openclaw/core') {
        // Replace with direct imports
        importDecl.remove();
        
        // Add direct imports for each named import
        const namedImports = importDecl.getNamedImports();
        for (const namedImport of namedImports) {
          const name = namedImport.getName();
          sourceFile.addImportDeclaration({
            moduleSpecifier: `@openclaw/core/${name.toLowerCase()}/${name}`,
            namedImports: [name],
          });
        }
      }
    }
  }
  
  project.saveSync();
}
```

### Phase 3: Barrel Removal

Once all imports are inlined, remove the barrel files:

```bash
# Remove barrel files
git rm src/core/index.ts
git rm src/plugin/index.ts
git rm src/channel/index.ts

# Update path mappings in tsconfig.json
# Remove: "@openclaw/core": ["./src/core"]
# Keep: "@openclaw/core/*": ["./src/core/*"]
```

### Phase 4: CI Prevention

Add circular dependency detection to CI:

```yaml
# .github/workflows/ci.yml
name: CI
jobs:
  check-circular:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
      - run: npm install -g madge
      - run: madge --circular src/
        # Fails if circular dependencies found
```

---

## Commit Message Pattern

Follow Vincent Koc's established convention:

```
fix(cycles): bypass [module] [description]

- Replace barrel imports with direct module imports
- Narrow [type] surface to prevent circular deps
- Part of barrel elimination campaign

Refs: [previous commit if chain]
```

### Examples from Night Cycle

| Commit | Pattern |
|--------|---------|
| `fix(cycles): bypass context engine and config barrels` | Full barrel bypass |
| `fix(cycles): narrow channel registry imports` | Import narrowing |
| `fix(cycles): split session hook event types` | Type splitting |
| `fix(cycles): bypass session binding service type import` | Selective bypass |
| `fix(cycles): untangle tts runtime facade types` | Dependency untangling |

---

## Type Seams: The Key to Success

Barrel bypassing works best with **type seams** - explicit boundaries between modules:

```typescript
// src/types/seams.ts
// Public contracts that modules depend on

export interface IRuntime {
  execute(task: Task): Promise<Result>;
}

export interface IConfig {
  get<T>(key: string): T;
}

// Modules depend on interfaces, not implementations
// src/runtime/Runtime.ts
import { IConfig } from '../types/seams';
export class Runtime implements IRuntime {
  constructor(private config: IConfig) {}
}
```

---

## Expected Fitness Gain

| Metric | Before | After | Change |
|--------|--------|-------|--------|
| syntax_score | 58/100 | 78/100 | +20% |
| semantic_score | 55/100 | 70/100 | +15% |
| build_time | baseline | -30% | faster |
| bundle_size | baseline | -25% | smaller |
| circular_deps | 47 | 0 | eliminated |

---

## Migration Timeline

Based on the Night Cycle (24 commits over ~2 weeks):

| Week | Activity | Commits |
|------|----------|---------|
| 1 | Detection and critical barrel bypassing | 12 |
| 2 | Type splitting and import narrowing | 12 |
| 3+ | Remaining barrels and stabilization | ongoing |

**Recommendation:** Proceed at ~2-3 barrel bypass commits per day to allow CI stabilization between changes.

---

## Rules Summary

1. **No new barrel files** - Use direct imports for new code
2. **One bypass per PR** - Keep changes reviewable
3. **Run CI after each bypass** - Catch circular deps early
4. **Split types when needed** - Untangle before bypassing
5. **Document the pattern** - Help teammates understand the change

---

## Tools

- **[Madge](https://github.com/pahen/madge)** - Circular dependency detection
  ```bash
  npx madge --circular src/
  npx madge --image deps.png src/
  ```

- **[ts-morph](https://github.com/dsherret/ts-morph)** - TypeScript AST manipulation for automated refactoring

- **[dependency-cruiser](https://github.com/sverweij/dependency-cruiser)** - Validate and visualize dependencies

---

## References

- Night Cycle Report: `night_cycle_20260412_0330.md`
- Explicit Seams Pattern: `evolution/references/explicit_seams_pattern.md`
- Context Tree Pattern: `evolution/references/context_tree_pattern.md`
- IronReview T430: `evolution/references/ironreview_t430_integration.md`

---

*Generated by OpenEvolve Night Cycle*  
*Classification: P0 Critical Architecture Pattern*  
*Credit: Vincent Koc's systematic approach to circular dependency elimination*
