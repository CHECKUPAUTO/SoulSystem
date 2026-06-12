# Pure Test Migration Pattern

## Pattern Overview

**Name:** Pure Test Migration ("Purity-Seeking" Refactoring)
**Source:** Commits 7e66a8fcfe through 36c412d81e (April 10-11, 2026)
**T430 Fitness Score:** 0.93 (Tier 1)

## Problem

Integration tests with I/O dependencies (file system, network, database) are:
- Slow (seconds vs milliseconds)
- Flaky (dependent on environment state)
- Hard to parallelize
- Resource intensive

## Solution

Extract business logic into pure functions that accept dependencies as parameters, then test with mocked dependencies.

## Migration Template

### Step 1: Identify I/O-Bound Logic

```typescript
// BEFORE: Integration test with real dependencies
// src/cli/plugins-cli.list.test.ts
import { test, expect } from "vitest";
import { pluginsCli } from "./plugins-cli";

test("plugin list formats correctly", async () => {
  // Real filesystem access, slow
  const result = await pluginsCli.list({ format: "json" });
  expect(result).toMatchSnapshot();
});
```

### Step 2: Extract Pure Function

Create a helper file with pure logic:

```typescript
// src/cli/plugins-list-format.ts
export interface PluginInfo {
  name: string;
  version: string;
  enabled: boolean;
}

export interface FormatOptions {
  format: "json" | "table" | "yaml";
}

// Pure function - no I/O, no side effects
export function formatPluginList(
  plugins: PluginInfo[],
  options: FormatOptions
): string {
  switch (options.format) {
    case "json":
      return JSON.stringify(plugins, null, 2);
    case "yaml":
      return plugins.map(p => `${p.name}: ${p.version}`).join("\n");
    case "table":
      return renderTable(plugins);
    default:
      throw new Error(`Unknown format: ${options.format}`);
  }
}

function renderTable(plugins: PluginInfo[]): string {
  // Pure table formatting logic
  return plugins.map(p => `${p.name.padEnd(20)} ${p.version}`).join("\n");
}
```

### Step 3: Create Pure Test

```typescript
// src/cli/plugins-list-format.test.ts
import { test, expect } from "vitest";
import { formatPluginList } from "./plugins-list-format";

test("formats empty list as JSON", () => {
  const result = formatPluginList([], { format: "json" });
  expect(result).toBe("[]");
});

test("formats plugins as JSON", () => {
  const plugins = [
    { name: "test-plugin", version: "1.0.0", enabled: true }
  ];
  const result = formatPluginList(plugins, { format: "json" });
  expect(JSON.parse(result)).toHaveLength(1);
});

test("formats plugins as table", () => {
  const plugins = [
    { name: "test-plugin", version: "1.0.0", enabled: true }
  ];
  const result = formatPluginList(plugins, { format: "table" });
  expect(result).toContain("test-plugin");
});
```

### Step 4: Create Runtime Wrapper (Optional)

If the original function needs to remain public:

```typescript
// src/cli/plugins-cli.ts
import { formatPluginList } from "./plugins-list-format";
import { loadPluginsFromDisk } from "./plugin-loader";

// Runtime version that does I/O
export async function list(options: FormatOptions): Promise<string> {
  const plugins = await loadPluginsFromDisk(); // I/O here
  return formatPluginList(plugins, options);     // Pure logic
}
```

### Step 5: Delete Integration Test

Remove the slow I/O-bound test file.

## Naming Conventions

| Old Pattern | New Pattern |
|-------------|-------------|
| `*.test.ts` (integration) | `*.pure.test.ts` or `*.helpers.ts` |
| `src/cli/plugins-cli.list.test.ts` | `src/cli/plugins-list-format.test.ts` |

## Files Affected (Reference)

Successfully migrated:
- `src/cli/plugins-cli.list.test.ts` → `src/cli/plugins-list-format.test.ts`
- `src/cli/plugins-cli.uninstall.test.ts` → `src/cli/plugins-uninstall-selection.test.ts`
- `src/cli/plugins-cli.update.test.ts` → `src/cli/plugins-update-selection.test.ts`
- `src/agents/openclaw-tools.subagents.sessions-spawn-default-timeout*.test.ts` → Deleted (pure coverage)
- `src/agents/sessions-spawn-threadid.test.ts` → Deleted (pure coverage)

## Benefits

1. **Speed:** Pure tests run in milliseconds vs seconds
2. **Reliability:** No environment dependencies
3. **Parallelization:** Tests can run in parallel without conflicts
4. **Clarity:** Tests focus on logic, not setup/teardown
5. **Maintainability:** Easier to understand and modify

## When NOT to Use

- Testing actual I/O behavior (network timeouts, disk errors)
- Testing integration points between systems
- End-to-end workflows

## Related Patterns

- **Barrel Avoidance** (`plugin_avoidance_pattern_2026-04-11.md`): Splitting modules for direct imports
- **Test Seams**: Using `.runtime.ts` suffix for I/O-bound modules

## T430 Score Impact

| Metric | Before | After | Change |
|--------|--------|-------|--------|
| Syntax | 1.0 | 1.0 | - |
| Semantic | 0.85 | 0.95 | +0.10 |
| Quality | 0.75 | 0.90 | +0.15 |
| Security | 0.80 | 0.85 | +0.05 |
| **Total** | **0.85** | **0.93** | **+0.08** |
