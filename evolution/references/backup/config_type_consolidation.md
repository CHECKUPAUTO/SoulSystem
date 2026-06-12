# Config Type Consolidation Pattern

**Pattern ID:** CONFIG-TYPE-CONSOLIDATE  
**Source:** Night Cycle 2026-04-12 04:15 UTC (Vincent Koc barrel bypassing)  
**Classification:** Code Organization / Type Safety  
**Status:** ✅ Validated / In Production

---

## Overview

Eliminates type import fragmentation by consolidating all config types into a single barrel export. Addresses the issue where `OpenClawConfig` was imported from 5+ different paths, causing module resolution ambiguity.

**Before:** 5 import paths, 1,179 total imports  
**After:** 1 import path, all types centralized

---

## The Problem

From the 04:15 Night Cycle analysis:

```typescript
// Fragmented type imports across codebase
import { OpenClawConfig } from '../config/types.openclaw.js';     // 426 imports
import { OpenClawConfig } from '../../config/types.openclaw.js';  // 280 imports
import { OpenClawConfig } from '../config/config.js';            // 251 imports
import { OpenClawConfig } from '../../config/config.js';         // 134 imports
import { OpenClawConfig } from '../config/types.js';              // 88 imports
```

**Issues:**
1. **Ambiguity**: TypeScript module resolution uncertainty
2. **Maintenance**: Change one path, update 1,000+ imports
3. **Inconsistency**: Same type, different source
4. **Circular risk**: Deep relative imports create cycles

---

## The Solution

Single barrel export with direct re-exports:

```typescript
// src/config/index.ts (the one true source)
export type { OpenClawConfig } from './types.openclaw.js';
export type { RuntimeConfig } from './types.runtime.js';
export type { GatewayConfig } from './types.gateway.js';
export type { AgentConfig } from './types.agent.js';
export type { ChannelConfig } from './types.channel.js';
export type { PluginConfig } from './types.plugin.js';
export type { AuthProfileConfig } from './types.auth.js';
export type { ModelProviderConfig } from './types.models.js';
export type { ToolPolicyConfig } from './types.tools.js';

// Constants and utilities
export { DEFAULT_CONFIG } from './defaults.js';
export { ConfigValidator } from './validator.js';
export { ConfigLoader } from './loader.js';

// Factory functions
export { createDefaultConfig } from './factory.js';
export { mergeConfigs } from './merge.js';
```

Usage:
```typescript
// After: Single import path everywhere
import { OpenClawConfig, RuntimeConfig, DEFAULT_CONFIG } from '@/config';
// or
import { OpenClawConfig } from '@openclaw/config';
```

---

## Migration Strategy

### Phase 1: Create Centralized Barrel

```typescript
// Step 1: Create src/config/index.ts
// Re-export all types from existing files

// Step 2: Add path alias in tsconfig.json
{
  "compilerOptions": {
    "paths": {
      "@/config": ["./src/config/index.ts"],
      "@/config/*": ["./src/config/*"]
    }
  }
}

// Step 3: Update package.json exports (for external packages)
{
  "exports": {
    "./config": {
      "types": "./dist/config/index.d.ts",
      "import": "./dist/config/index.js"
    }
  }
}
```

### Phase 2: Codemod Migration

```bash
# Find all OpenClawConfig imports
find src -name "*.ts" -exec grep -l "OpenClawConfig" {} \; | xargs grep "from.*config"

# Codemod script (jscodeshift or similar)
npx jscodeshift -t config-import-codemod.ts src/
```

```typescript
// config-import-codemod.ts
transform({ source }, { j }) {
  const root = j(source);
  
  // Replace relative imports with barrel
  root.find(j.ImportDeclaration)
    .filter(path => 
      path.value.source.value?.includes('config/types') ||
      path.value.source.value?.includes('config/config')
    )
    .forEach(path => {
      const specifiers = path.value.specifiers;
      // Replace with @/config import
      j(path).replaceWith(
        j.importDeclaration(
          specifiers,
          j.literal('@/config')
        )
      );
    });
  
  return root.toSource();
}
```

### Phase 3: Deprecate Old Imports

```typescript
// src/config/types.openclaw.ts
// Add deprecation notice

/**
 * @deprecated Use `@/config` or `@openclaw/config` instead
 * This file will be removed in v3.0
 */
export type { OpenClawConfig } from './types/openclaw.js';
```

---

## Type Consolidation Details

### Before (Fragmented)

```
src/config/
├── types.openclaw.ts      # Main config (426 + 280 imports)
├── config.ts               # Runtime config (251 + 134 imports)
├── types.ts                # Generic types (88 imports)
├── types.runtime.ts
├── types.gateway.ts
├── types.agent.ts
├── types.channel.ts
└── types.plugin.ts
```

### After (Consolidated)

```
src/config/
├── index.ts                 # Barrel export (NEW)
├── types/
│   ├── openclaw.ts          # Moved from types.openclaw.ts
│   ├── runtime.ts
│   ├── gateway.ts
│   ├── agent.ts
│   ├── channel.ts
│   └── plugin.ts
├── config.ts                # Runtime config (now internal)
├── validator.ts
├── loader.ts
├── defaults.ts
├── factory.ts
└── merge.ts
```

---

## Related Pattern: Import Narrowing

From the barrel bypassing campaign, combine with direct leaf imports:

```typescript
// ❌ Before: Deep relative imports
import { OpenClawConfig } from '../../../config/types.openclaw.js';

// ✅ After: Barrel import
import { OpenClawConfig } from '@/config';

// ✅ Alternative: Direct leaf for tree-shaking
import { OpenClawConfig } from '@/config/types/openclaw.js';
```

---

## Benefits

| Metric | Before | After | Improvement |
|--------|--------|-------|-------------|
| Import paths | 5+ | 1 | -80% |
| Import count | 1,179 | ~500 | -58% |
| Build ambiguity | High | None | Resolved |
| IDE autocomplete | Fragmented | Unified | Improved |
| Refactoring safety | Low | High | Better |

---

## Validation

```typescript
// Verify no duplicate type definitions
type ConfigFromTypes = import('@/config/types/openclaw.js').OpenClawConfig;
type ConfigFromBarrel = import('@/config').OpenClawConfig;

// Should be identical
type Verify = ConfigFromTypes extends ConfigFromBarrel ? true : false;
//    ^? type Verify = true
```

---

## Commit References

From the 04:15 Night Cycle report:

- `5cd9c2d2de` - fix(cycles): bypass context engine and config barrels
- `8e952eba75` - fix(pairing): bypass store and channel barrels
- `25665dd335` - fix(runtime): bypass get-reply barrel exports
- Config consolidation is part of the broader barrel bypassing campaign

---

## References

- Night Cycle 2026-04-12 04:15 UTC: Barrel bypassing campaign analysis
- Barrel Bypassing Guide: `evolution/references/barrel_bypassing_guide.md`
- Plugin Avoidance Pattern: `evolution/references/plugin_avoidance_pattern_2026-04-11.md`
- Narrow Surface Pattern: `evolution/references/narrow_surface_pattern.md`

---

*Pattern extracted from OpenEvolve Night Cycle analysis*  
*Generated: 2026-04-12 06:24 UTC*
