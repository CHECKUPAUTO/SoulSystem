# Pure Test Migration Pattern v2

**Source:** OpenEvolve Night Cycle Report 2026-04-12 (0019)  
**Purpose:** Complete pattern guide for migrating I/O-bound integration tests to pure unit tests with template and CI configuration

## Overview

This pattern documents the systematic migration observed in commits 7e66a8fcfe through 36c412d81e, where I/O-bound test logic was extracted into isolated, mock-based unit tests.

## Pattern Template

### Step 1: Identify I/O-Bound Test

```typescript
// Before: Integration test with I/O dependencies
// src/agents/openclaw-tools.subagents.sessions-spawn-default-timeout.test.ts

describe('Sessions Spawn', () => {
  it('respects timeout with real store', async () => {
    const store = createMemoryStore(); // I/O dependency
    await store.connect(); // Slow
    
    const result = await spawnAgent({ timeout: 5000 });
    expect(result.timedOut).toBe(false);
    
    await store.disconnect(); // Cleanup
  });
});
```

### Step 2: Extract Pure Logic

```typescript
// src/agents/sessions/spawn-timeout-calculation.ts

export interface SpawnTimeoutConfig {
  defaultTimeout: number;
  minTimeout: number;
  maxTimeout: number;
  userOverride?: number;
}

export function calculateSpawnTimeout(
  config: SpawnTimeoutConfig
): number {
  const requested = config.userOverride ?? config.defaultTimeout;
  return Math.min(Math.max(requested, config.minTimeout), config.maxTimeout);
}

export function shouldWarnShortTimeout(timeout: number): boolean {
  return timeout < 5000;
}
```

### Step 3: Create Pure Test

```typescript
// src/agents/sessions/spawn-timeout-calculation.test.ts

import { describe, it, expect } from 'vitest';
import { calculateSpawnTimeout, shouldWarnShortTimeout } from './spawn-timeout-calculation';

/**
 * @pure
 * Tests timeout calculation logic without I/O dependencies
 */
describe('Spawn Timeout Calculation', () => {
  const defaultConfig = {
    defaultTimeout: 30000,
    minTimeout: 1000,
    maxTimeout: 300000,
  };

  it('uses default when no override provided', () => {
    const result = calculateSpawnTimeout(defaultConfig);
    expect(result).toBe(30000);
  });

  it('clamps to minimum', () => {
    const result = calculateSpawnTimeout({
      ...defaultConfig,
      userOverride: 500,
    });
    expect(result).toBe(1000);
  });

  it('clamps to maximum', () => {
    const result = calculateSpawnTimeout({
      ...defaultConfig,
      userOverride: 600000,
    });
    expect(result).toBe(300000);
  });

  it('warns on short timeouts', () => {
    expect(shouldWarnShortTimeout(3000)).toBe(true);
    expect(shouldWarnShortTimeout(10000)).toBe(false);
  });
});
```

### Step 4: Simplify Integration Test

```typescript
// After: Integration test delegates to pure logic
// src/agents/sessions/spawn.integration.test.ts

import { calculateSpawnTimeout } from './spawn-timeout-calculation';

describe('Sessions Spawn (Integration)', () => {
  it('applies timeout configuration', async () => {
    // Mock the store - no real I/O
    const store = createMockStore();
    
    // Use pure logic for timeout
    const timeout = calculateSpawnTimeout({
      defaultTimeout: 5000,
      minTimeout: 1000,
      maxTimeout: 60000,
    });
    
    const result = await spawnAgent({ timeout, store: mockStore });
    expect(result.timeout).toBe(5000);
  });
});
```

## CI Configuration

```yaml
# .github/workflows/test.yml
name: Tests

on: [push, pull_request]

jobs:
  pure-tests:
    name: Pure Tests (Fast)
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
      - run: npm ci
      - run: npm run test:pure -- --reporter=dot
      - run: npm run test:pure -- --coverage

  integration-tests:
    name: Integration Tests
    runs-on: ubuntu-latest
    needs: pure-tests
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
      - run: npm ci
      - run: npm run test:integration
```

## Migration Checklist

- [ ] Identify I/O-bound test assertions
- [ ] Extract pure function to `*.helpers.ts` or `*.logic.ts`
- [ ] Create `*.pure.test.ts` with mocked dependencies
- [ ] Update imports to use direct helper imports
- [ ] Delete original I/O test or simplify to mock-based
- [ ] Add `@pure` annotation to new test file
- [ ] Verify test passes in CI

## Benefits

| Metric | Before | After | Improvement |
|--------|--------|-------|-------------|
| Execution Time | ~2s/test | ~50ms/test | 40x faster |
| CI Flakiness | High | Low | Stable |
| Parallelization | Limited | Full | Better |
| Debug Feedback | Delayed | Immediate | Faster |

## Common Extractable Patterns

### Timeout Logic
```typescript
// Extract: timeout calculation, retry logic
// Mock: clock, scheduler
```

### Data Validation
```typescript
// Extract: schema validation, format checking
// Mock: validator functions
```

### Selection Logic
```typescript
// Extract: item selection algorithms
// Mock: candidate lists
```

## References

- Night Cycle Report: night_cycle_20260412_0019.md
- Commits: 7e66a8fcfe, 5ca92b0498, 7273cae36b, 36c412d81e
- Related: test_categorization_annotations.md
