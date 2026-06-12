# Narrow Surface Pattern

**Source:** OpenEvolve Night Cycle Report 2026-04-12 03:30 UTC  
**Priority:** P3 - Medium  
**Use Case:** Prevent breaking changes through minimal API surface area

---

## Problem Statement

Over-exported types in the plugin SDK create:
- Breaking change risks with every update
- Consumer confusion about what to use
- Difficulty evolving internal implementations
- Tight coupling between modules

**Evidence from Night Cycle:**
- Multiple "narrow" and "split" commits in plugin-sdk
- `fix(cycles): narrow provider runtime error hook types`
- `fix(cycles): narrow channel registry imports`
- `fix(cycles): split command flag helpers`

**Update 2026-04-12 23:56:**
- Feishu `comment-shared.ts` at 331 lines — growing barrel risk. Recommend splitting into `comment-types.ts`, `comment-parser.ts`, `comment-formatter.ts`
- Codex OAuth scope handler (287 lines of tests) — candidate for scope-manager extraction with pure tests
- Video generation `types.ts` added 47 lines — provider registry pattern could prevent scattered imports (14 providers now)

---

## Solution: Minimal API Surface

Export only what callers absolutely need:

### BEFORE: Over-Exporting

```typescript
// src/runtime/index.ts - exports everything
export * from './Runtime';
export * from './RuntimeConfig';
export * from './RuntimeInternals';
export * from './RuntimeHelpers';
export * from './RuntimeState';
export * from './RuntimeErrors';

// Consumer sees everything (confusing)
import { Runtime, RuntimeConfig, RuntimeInternals } from '@openclaw/runtime';
// Which ones are public API vs internal?
```

### AFTER: Narrow Surface

```typescript
// src/runtime/public.ts - curated public API
export { Runtime } from './Runtime';
export type { RuntimeConfig } from './RuntimeConfig';
export { RuntimeError } from './RuntimeErrors';

// NOT exported:
// - RuntimeInternals (implementation detail)
// - RuntimeHelpers (internal utilities)
// - RuntimeState (internal state management)

// Consumer sees only public API (clear)
import { Runtime, RuntimeConfig } from '@openclaw/runtime';
```

---

## Implementation Strategy

### Rule 1: Prefer interfaces over concrete classes in exports

```typescript
// BEFORE: Export concrete class
export class Runtime {
  constructor(config: RuntimeConfig) { ... }
  execute(task: Task): Promise<Result> { ... }
}

// AFTER: Export interface + factory
export interface IRuntime {
  execute(task: Task): Promise<Result>;
}

export function createRuntime(config: RuntimeConfig): IRuntime {
  return new RuntimeImpl(config);
}

// Implementation stays internal
class RuntimeImpl implements IRuntime { ... }
```

### Rule 2: Use `type` exports over `class` exports where possible

```typescript
// BEFORE: Export class as value
export class Config { ... }

// AFTER: Export type only (if class not needed)
export type Config = {
  name: string;
  version: string;
};

// Or both if needed
export type Config = InstanceType<typeof ConfigImpl>;
export function createConfig(...): Config { ... }
```

### Rule 3: Split "public API" from "internal implementation" at file level

```
src/runtime/
├── public.ts           # Public API (narrow surface)
├── Runtime.ts          # Implementation
├── RuntimeConfig.ts    # Config types
├── internal/
│   ├── RuntimeInternals.ts    # Internal details
│   ├── RuntimeHelpers.ts      # Internal utilities
│   └── RuntimeState.ts        # Internal state
└── index.ts            # Re-export only public.ts
```

### Rule 4: Review surface area quarterly

```typescript
// scripts/analyze-surface.ts

import { Project } from 'ts-morph';

function analyzeApiSurface(projectPath: string) {
  const project = new Project({ tsConfigFilePath: `${projectPath}/tsconfig.json` });
  
  const publicApis = project.getSourceFiles()
    .filter(f => f.getFilePath().includes('public.ts'));
  
  for (const api of publicApis) {
    const exports = api.getExportedDeclarations();
    console.log(`\n${api.getBaseName()}:`);
    for (const [name, nodes] of exports) {
      console.log(`  - ${name} (${nodes[0].getKindName()})`);
    }
  }
}

// Run quarterly: npm run analyze:api-surface
```

---

## Type Narrowing Examples

### Splitting Monolithic Types

```typescript
// BEFORE: One large type file
export interface ProviderRuntime {
  config: ProviderConfig;
  hooks: ProviderHooks;
  errors: ProviderErrors;
  state: ProviderState;
}

// AFTER: Split by concern, export only needed parts
// src/provider/types/ProviderConfig.ts
export interface ProviderConfig { ... }

// src/provider/types/ProviderHooks.ts  
export interface ProviderHooks { ... }

// src/provider/public.ts
export { ProviderConfig } from './types/ProviderConfig';
export type { ProviderHooks } from './types/ProviderHooks';
// NOT exported: ProviderState, ProviderErrors (internal)
```

### Narrowing Error Types

```typescript
// BEFORE: Broad error type
export interface ProviderRuntimeErrors {
  connection: ConnectionError;
  authentication: AuthError;
  rateLimit: RateLimitError;
  internal: InternalError;
}

// AFTER: Narrow to what consumers need
export type ProviderError =
  | { type: 'connection'; message: string }
  | { type: 'authentication'; message: string };

// Internal errors not exposed
// - RateLimitError (handled internally)
// - InternalError (logged, not thrown)
```

---

## Plugin SDK Application

### Before: Broad Exports

```typescript
// extensions/plugin-sdk/src/index.ts
export * from './Plugin';
export * from './PluginConfig';
export * from './PluginHooks';
export * from './PluginState';
export * from './PluginErrors';
export * from './PluginRegistry';
export * from './PluginLoader';
```

### After: Narrow Surface

```typescript
// extensions/plugin-sdk/src/public.ts

// Core plugin interface (required)
export { Plugin } from './Plugin';
export type { PluginConfig } from './PluginConfig';

// Lifecycle hooks (optional)
export type { PluginHooks } from './PluginHooks';

// Registration (for plugin authors)
export { registerPlugin } from './PluginRegistry';

// NOT exported:
// - PluginState (internal state)
// - PluginErrors (internal error handling)
// - PluginLoader (internal loading mechanism)
// - PluginRegistry internals (use registerPlugin instead)
```

---

## ESLint Enforcement

```javascript
// .eslintrc.js
module.exports = {
  rules: {
    // Prevent implementation exports from public files
    'no-restricted-syntax': [
      'error',
      {
        selector: 'ExportNamedDeclaration[source.value*="/internal/"]',
        message: 'Do not export from internal modules in public files'
      }
    ],
    
    // Enforce type exports
    '@typescript-eslint/consistent-type-exports': 'error',
    
    // Prefer interface over class in public API
    'no-restricted-globals': [
      'error',
      {
        name: 'class',
        message: 'Prefer interfaces over classes in public API'
      }
    ]
  }
};
```

---

## Expected Fitness Gain

| Metric | Before | After | Change |
|--------|--------|-------|--------|
| quality_score | 70/100 | 85/100 | +15% |
| breaking changes | frequent | rare | improved |
| consumer confusion | high | low | improved |
| internal flexibility | low | high | improved |

---

## Migration Checklist

- [ ] Identify all `export *` statements
- [ ] Audit which exports are actually used by consumers
- [ ] Create `public.ts` files with curated exports
- [ ] Replace `export *` with explicit exports
- [ ] Mark internal modules with `/** @internal */` JSDoc
- [ ] Add ESLint rules to prevent regression
- [ ] Document public API in README

---

## References

- Night Cycle Report: `night_cycle_20260412_0330.md`
- Plugin SDK Narrowing Commits: 15, 22, 25, 42, 48
- IronReview T430: `ironreview_t430_integration.md`

---

*Generated by OpenEvolve Night Cycle*  
*Classification: P3 Maintenance Pattern*
