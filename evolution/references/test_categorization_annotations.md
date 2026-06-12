# Test Categorization Annotations

**Source:** OpenEvolve Night Cycle Report 2026-04-11 (2115)
**Purpose:** Enable @pure, @integration, @e2e test annotations for CI filtering and faster runs

## Overview

Annotate tests by category to enable selective test execution and clearer intent.

## Annotations

### @pure - Unit Tests

Fast, deterministic tests with no I/O dependencies.

```typescript
/**
 * @pure
 * Tests logic with mocked dependencies.
 */
describe('ChannelUtils', () => {
  it('normalizes channel IDs', () => {
    expect(normalizeChannelId('  Test-123  ')).toBe('test-123');
  });
});
```

### @integration - Component Tests

Tests that cross module boundaries but don't require external services.

```typescript
/**
 * @integration
 * Tests channel registry with in-memory plugins.
 */
describe('ChannelRegistry', () => {
  it('loads bundled plugins', async () => {
    const registry = await createRegistry();
    expect(registry.get('telegram')).toBeDefined();
  });
});
```

### @e2e - End-to-End Tests

Full system tests with real external dependencies.

```typescript
/**
 * @e2e
 * Requires Telegram bot token and network access.
 */
describe('TelegramIntegration', () => {
  it('sends a message', async () => {
    // Real API call
  });
});
```

## Vitest Configuration

```typescript
// vitest.config.ts
import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    // Run all by default
    include: ['src/**/*.test.ts'],
    
    // Tag-based filtering
    tagFilter: process.env.TEST_TAG || '*',
    
    // Custom reporters for categorized output
    reporters: ['default', {
      name: 'categorized',
      summary: true,
    }],
  },
});
```

## NPM Scripts

```json
{
  "scripts": {
    "test": "vitest",
    "test:pure": "TEST_TAG=@pure vitest --tag @pure",
    "test:integration": "TEST_TAG=@integration vitest --tag @integration",
    "test:e2e": "TEST_TAG=@e2e vitest --tag @e2e",
    "test:ci": "vitest --tag @pure --tag @integration --exclude-tag @e2e",
    "test:fast": "npm run test:pure"
  }
}
```

## CI Configuration

```yaml
# .github/workflows/test.yml
name: Tests

on: [push, pull_request]

jobs:
  pure:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: npm ci
      - run: npm run test:pure

  integration:
    runs-on: ubuntu-latest
    needs: pure
    steps:
      - uses: actions/checkout@v4
      - run: npm ci
      - run: npm run test:integration

  e2e:
    runs-on: ubuntu-latest
    needs: integration
    if: github.ref == 'refs/heads/main'
    steps:
      - uses: actions/checkout@v4
      - run: npm ci
      - run: npm run test:e2e
        env:
          TELEGRAM_BOT_TOKEN: ${{ secrets.TELEGRAM_BOT_TOKEN }}
```

## Custom Decorator (Alternative)

```typescript
// test/decorators.ts
export function pure(target: any, propertyKey: string, descriptor: PropertyDescriptor) {
  descriptor.value._testCategory = '@pure';
}

export function e2e(target: any, propertyKey: string, descriptor: PropertyDescriptor) {
  descriptor.value._testCategory = '@e2e';
}
```

## Migration Path

1. Add annotations to existing tests
2. Update vitest.config.ts
3. Add npm scripts
4. Configure CI with stage gates
5. Document in CONTRIBUTING.md

## Benefits

1. **Faster Feedback:** Run only @pure in pre-commit hooks
2. **Resource Efficiency:** Skip @e2e on feature branches
3. **Clear Intent:** Know test scope at a glance
4. **Parallel CI:** Stages can run in parallel or sequence

## References

- Night Cycle Report: night_cycle_20260411_2115.md
- Related: pure_test_migration_pattern.md
