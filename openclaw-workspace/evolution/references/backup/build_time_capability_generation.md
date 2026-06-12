# Build-Time Capability Generation

**Source:** OpenEvolve Night Cycle Report 2026-04-11 (2115)
**Purpose:** Generate static capability tables from plugin definitions to prevent drift

## Problem Statement

`STATIC_DOCTOR_CHANNEL_CAPABILITIES` is manually maintained, leading to drift between:
- Plugin definitions (source of truth)
- Static lookup tables (potentially stale)
- Runtime registry (may have different values)

## Solution

Generate static tables at build time from actual plugin definitions.

## Implementation

### Generator Script

```typescript
// scripts/generate-channel-capabilities.ts
import * as fs from 'fs';
import * as path from 'path';
import { CHANNEL_PLUGINS } from '../src/channels/plugins/bundled';

interface CapabilityEntry {
  type: string;
  capabilities: {
    supportsReactions: boolean;
    supportsThreads: boolean;
    supportsPolls: boolean;
    supportsTyping: boolean;
    supportsEditing: boolean;
    supportsDeletion: boolean;
  };
}

function generateCapabilities(): string {
  const entries: CapabilityEntry[] = CHANNEL_PLUGINS.map(plugin => ({
    type: plugin.type,
    capabilities: {
      supportsReactions: plugin.capabilities?.supportsReactions ?? false,
      supportsThreads: plugin.capabilities?.supportsThreads ?? false,
      supportsPolls: plugin.capabilities?.supportsPolls ?? false,
      supportsTyping: plugin.capabilities?.supportsTyping ?? true,
      supportsEditing: plugin.capabilities?.supportsEditing ?? true,
      supportsDeletion: plugin.capabilities?.supportsDeletion ?? true,
    },
  }));

  const output = `// AUTO-GENERATED: Do not edit manually
// Generated at: ${new Date().toISOString()}
// Source: src/channels/plugins/bundled.ts

export const STATIC_DOCTOR_CHANNEL_CAPABILITIES = {
${entries.map(e => `  '${e.type}': ${JSON.stringify(e.capabilities)},`).join('\n')}
} as const;

export type ChannelCapabilityType = keyof typeof STATIC_DOCTOR_CHANNEL_CAPABILITIES;
`;

  return output;
}

function main(): void {
  const outputPath = path.join(__dirname, '../src/generated/channel-capabilities.ts');
  
  // Ensure directory exists
  const dir = path.dirname(outputPath);
  if (!fs.existsSync(dir)) {
    fs.mkdirSync(dir, { recursive: true });
  }
  
  const content = generateCapabilities();
  fs.writeFileSync(outputPath, content, 'utf-8');
  
  console.log(`Generated: ${outputPath}`);
  console.log(`Channels: ${CHANNEL_PLUGINS.length}`);
}

main();
```

### Package.json Scripts

```json
{
  "scripts": {
    "generate": "tsx scripts/generate-channel-capabilities.ts",
    "build": "npm run generate && tsc",
    "watch": "nodemon --watch src/channels/plugins --exec 'npm run generate'",
    "test": "npm run generate && vitest"
  }
}
```

### CI Integration

```yaml
# .github/workflows/ci.yml
name: CI

on: [push, pull_request]

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
      - run: npm ci
      
      # Generate capabilities
      - run: npm run generate
      
      # Verify no drift
      - name: Check generated files
        run: |
          if [ -n "$(git status --porcelain src/generated/)" ]; then
            echo "Error: Generated files are out of date"
            git diff src/generated/
            exit 1
          fi
      
      - run: npm run build
      - run: npm test
```

## Pre-commit Hook

```bash
#!/bin/sh
# .husky/pre-commit

# Generate capabilities
npm run generate

# Stage generated files
git add src/generated/

# Check for drift
if [ -n "$(git diff --cached --name-only src/generated/)" ]; then
  echo "Generated channel capabilities updated"
fi
```

## Generated File Structure

```typescript
// src/generated/channel-capabilities.ts
// AUTO-GENERATED: Do not edit manually
// Generated at: 2026-04-11T21:15:00Z
// Source: src/channels/plugins/bundled.ts

export const STATIC_DOCTOR_CHANNEL_CAPABILITIES = {
  'telegram': {
    "supportsReactions": true,
    "supportsThreads": true,
    "supportsPolls": true,
    "supportsTyping": true,
    "supportsEditing": true,
    "supportsDeletion": true
  },
  'discord': {
    "supportsReactions": true,
    "supportsThreads": true,
    "supportsPolls": true,
    "supportsTyping": true,
    "supportsEditing": true,
    "supportsDeletion": true
  },
  // ... other channels
} as const;

export type ChannelCapabilityType = keyof typeof STATIC_DOCTOR_CHANNEL_CAPABILITIES;
```

## Consumption Pattern

```typescript
// src/commands/doctor/channel-capabilities.ts
import { STATIC_DOCTOR_CHANNEL_CAPABILITIES, ChannelCapabilityType } from '../../generated/channel-capabilities';
import { createStaticLookup } from '../../utils/static-lookup';

const channelCapabilities = createStaticLookup(
  STATIC_DOCTOR_CHANNEL_CAPABILITIES,
  {
    fallback: {
      supportsReactions: false,
      supportsThreads: false,
      supportsPolls: false,
      supportsTyping: false,
      supportsEditing: false,
      supportsDeletion: false,
    },
    keyTransform: (k) => k.toLowerCase()
  }
);

export function getChannelCapabilities(channelType: string) {
  return channelCapabilities.get(channelType);
}
```

## Benefits

1. **Single Source of Truth:** Plugin definitions drive everything
2. **Type Safety:** Generated TypeScript types
3. **No Drift:** CI ensures generated files are up-to-date
4. **Compile-Time Checks:** TypeScript catches missing capabilities

## Migration Path

1. Create `scripts/generate-channel-capabilities.ts`
2. Run generator to create initial file
3. Update imports to use generated file
4. Add CI check
5. Remove manual `STATIC_DOCTOR_CHANNEL_CAPABILITIES`

## References

- Night Cycle Report: night_cycle_20260411_2115.md
- Related: create_static_lookup_utility.md
