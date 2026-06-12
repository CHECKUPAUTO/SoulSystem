# Static Capability Generation

**Source:** OpenEvolve Night Cycle Report 2026-04-12 01:15 UTC
**Priority:** P1 - Performance Optimization
**Expected Impact:** 20-40% reduction in tool lookup latency

---

## Problem Statement

Current OpenClaw performs dynamic plugin lookups for every tool invocation. This adds unnecessary latency for capabilities that are known at build time.

**Current Flow:**
```
Request → Dynamic Registry Lookup → Plugin Discovery → Tool Execution
         (O(n) scan)              (I/O)
```

**Target Flow:**
```
Request → Static Lookup Table → Tool Execution
         (O(1) array access)
```

---

## Implementation

### Build Script

```typescript
// scripts/generate-capabilities.ts
import { glob } from 'glob';
import { readFile } from 'fs/promises';
import { writeFileSync } from 'fs';
import { join } from 'path';

interface Capability {
  name: string;
  plugin: string;
  method: string;
  parameters: Parameter[];
}

interface Parameter {
  name: string;
  type: string;
  required: boolean;
}

async function generateCapabilities() {
  const capabilities: Map<string, Capability> = new Map();
  
  // Scan all plugin definitions
  const pluginFiles = await glob('src/plugins/**/plugin.json');
  
  for (const file of pluginFiles) {
    const content = await readFile(file, 'utf-8');
    const plugin = JSON.parse(content);
    
    for (const capability of plugin.capabilities || []) {
      capabilities.set(capability.name, {
        name: capability.name,
        plugin: plugin.name,
        method: capability.method,
        parameters: capability.parameters
      });
    }
  }
  
  // Generate TypeScript
  const generated = generateTypeScript(capabilities);
  
  writeFileSync(
    'src/generated/capabilities.ts',
    generated
  );
  
  console.log(`Generated ${capabilities.size} capabilities`);
}

function generateTypeScript(capabilities: Map<string, Capability>): string {
  const entries = Array.from(capabilities.entries());
  
  return `// AUTO-GENERATED: Do not edit manually
// Generated at ${new Date().toISOString()}

export interface Capability {
  name: string;
  plugin: string;
  method: string;
  parameters: Parameter[];
}

export interface Parameter {
  name: string;
  type: string;
  required: boolean;
}

// Static capability lookup table
export const STATIC_CAPABILITIES: Record<string, Capability> = {
${entries.map(([name, cap]) => `  ${name}: {
    name: '${cap.name}',
    plugin: '${cap.plugin}',
    method: '${cap.method}',
    parameters: ${JSON.stringify(cap.parameters)}
  }`).join(',\n')}
} as const;

// Type-safe capability names
export type CapabilityName = keyof typeof STATIC_CAPABILITIES;

// O(1) capability lookup
export function getCapability(name: CapabilityName): Capability {
  const capability = STATIC_CAPABILITIES[name];
  if (!capability) {
    throw new Error(\`Unknown capability: \${name}\`);
  }
  return capability;
}

// Fast path check
export function hasCapability(name: string): name is CapabilityName {
  return name in STATIC_CAPABILITIES;
}
`;
}

generateCapabilities().catch(console.error);
```

### Runtime Integration

```typescript
// src/tools/capability-resolver.ts
import { STATIC_CAPABILITIES, hasCapability, getCapability } from '../generated/capabilities';

export class CapabilityResolver {
  // Fast path: static lookup
  resolve(toolName: string): ResolvedTool {
    if (hasCapability(toolName)) {
      // O(1) lookup from generated table
      const capability = getCapability(toolName);
      return {
        source: 'static',
        plugin: capability.plugin,
        method: capability.method,
        parameters: capability.parameters
      };
    }
    
    // Fallback: dynamic discovery
    return this.dynamicResolve(toolName);
  }
  
  private dynamicResolve(toolName: string): ResolvedTool {
    // Legacy dynamic lookup
    // ...existing implementation
  }
}

interface ResolvedTool {
  source: 'static' | 'dynamic';
  plugin: string;
  method: string;
  parameters: unknown[];
}
```

---

## CI Integration

```yaml
# .github/workflows/generate-capabilities.yml
name: Generate Capabilities

on:
  push:
    paths:
      - 'src/plugins/**'
      - 'scripts/generate-capabilities.ts'

jobs:
  generate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      
      - uses: actions/setup-node@v3
        with:
          node-version: '20'
      
      - run: npm ci
      
      - run: npx tsx scripts/generate-capabilities.ts
      
      - name: Check for changes
        run: |
          if [[ -n $(git status --porcelain src/generated/capabilities.ts) ]]; then
            git config user.name "github-actions"
            git config user.email "actions@github.com"
            git add src/generated/capabilities.ts
            git commit -m "chore: regenerate capabilities [skip ci]"
            git push
          fi
```

---

## Expected Performance

| Metric | Before | After | Improvement |
|--------|--------|-------|-------------|
| Tool Lookup | O(n) scan | O(1) array | 20-40% |
| Cold Start | Includes scan | Static only | 10-15% |
| Memory | Dynamic registry | Static table | Slightly less |

---

## References

- Night Cycle Report: night_cycle_20260412_0115.md
- IronReview T430 Analysis