# Pattern: Parallel Test Seams (Platform-Specific Testing)

**Classification:** Testing Infrastructure Pattern | **Safety Level:** Documentation Only | **Source:** night_cycle_20260412_0301.md

## Overview

The Parallel Test Seams pattern enables platform-specific test isolation by extracting runtime-specific logic into separate modules, allowing tests to run cleanly across different platforms (Windows, macOS, Linux).

## Problem Statement

Running integration tests across multiple platforms (Parallels, Docker, CI) requires handling platform-specific behaviors:
- Different process management (Windows services vs Unix signals)
- Path handling differences
- Gateway lifecycle variations
- Browser automation nuances

## Pattern Structure

### File Naming Conventions

| Suffix | Purpose | Example |
|--------|---------|---------|
| `*.runtime.ts` | Runtime implementations | `gateway.runtime.ts` |
| `*-state.ts` | Extracted state | `store-lock-state.ts` |
| `*.lookup.test.ts` | Cache/lookup tests | `models-config.lookup.test.ts` |

### Platform-Specific Test Seams

```typescript
// test/parallels/gateway.runtime.ts
export interface GatewayRuntime {
  start(): Promise<void>;
  stop(): Promise<void>;
  isRunning(): boolean;
  getUrl(): string;
}

// Windows implementation
export const windowsGatewayRuntime: GatewayRuntime = {
  async start() { /* Windows service logic */ },
  async stop() { /* Windows service shutdown */ },
  isRunning() { /* Check Windows service */ },
  getUrl() { return "http://localhost:8080"; }
};

// macOS implementation  
export const macosGatewayRuntime: GatewayRuntime = {
  async start() { /* macOS process logic */ },
  async stop() { /* SIGTERM handling */ },
  isRunning() { /* Check process */ },
  getUrl() { return "http://localhost:8080"; }
};
```

## Real-World Examples

From recent OpenClaw commits:

### Gateway Listener Management
```typescript
// Commits: e08f4c12, 247705b5, 270a3999
// Pattern: Stop gateway before update, recover after

// test/parallels/gateway-lifecycle.test.ts
describe('Gateway Lifecycle', () => {
  const runtime = getPlatformRuntime(); // Windows or macOS
  
  beforeEach(async () => {
    await runtime.stopGateway();
  });
  
  afterEach(async () => {
    await runtime.startGateway();
  });
  
  test('npm update without conflicts', async () => {
    // Test runs with clean gateway state
  });
});
```

### Browser Substitution Prevention
```typescript
// Commit: e2046493
// Pattern: Avoid host Safari substitution on macOS

// test/parallels/browser.runtime.ts
export function getBrowserConfig(): BrowserConfig {
  if (process.platform === 'darwin') {
    return {
      executablePath: '/Applications/Brave Browser.app/Contents/MacOS/Brave Browser',
      avoidSubstitution: true // Prevent Safari hijacking
    };
  }
  // Windows/Linux configs...
}
```

## Benefits

1. **Platform Isolation** - Each platform has clean, testable runtime
2. **Reduced Flakiness** - No cross-platform interference
3. **Clear Failure Domains** - Platform-specific issues are isolated
4. **Reusable Runtimes** - Runtime modules can be reused across tests

## CodeWiki Entry

**Pattern ID:** `patterns/parallel-test-seams`  
**Related Patterns:**
- `runtime-extraction-pattern`
- `platform-abstraction-pattern`
- `test-fixture-pattern`

## Implementation Guidelines

### DO:
- Extract platform-specific logic to `.runtime.ts` files
- Use `withRuntimeState()` helper for isolated state testing
- Test each platform runtime independently
- Document platform-specific quirks

### DON'T:
- Inline platform checks in test logic
- Mix platform runtimes in same test
- Assume process management is identical across platforms

## Related Commits

| Commit | Description |
|--------|-------------|
| `e08f4c12` | Stop Windows gateway before update |
| `247705b5` | Bound macOS dashboard curl |
| `270a3999` | Recover macOS update gateway start |
| `e2046493` | Avoid host Safari substitution |
| `0e3f9657` | Preserve bundled host compatibility |

## References

- Test Mock Consolidation: `test_mock_consolidation_guide.md`
- Test Hardening Patterns: `test_hardening_patterns.md`
- Shared Test Fixtures: `shared_test_fixtures_library.md`

## Platform Runtime Registry

```typescript
// test/parallels/runtimes/index.ts
export const runtimes: Record<string, PlatformRuntime> = {
  win32: windowsRuntime,
  darwin: macosRuntime,
  linux: linuxRuntime,
};

export function getPlatformRuntime(): PlatformRuntime {
  const runtime = runtimes[process.platform];
  if (!runtime) {
    throw new Error(`Unsupported platform: ${process.platform}`);
  }
  return runtime;
}
```
