# Pure Test Template Guide
**Generated:** 2026-04-11 23:53 UTC
**Source:** night_cycle_20260411_2349.md
**Status:** ✅ Documented Best Practice

---

## Overview

The OpenClaw codebase is systematically migrating tests from E2E/integration to "pure" tests that focus on specific logic owners. This document provides a template for creating standardized pure tests.

---

## Pure Test Definition

A "pure" test is:
- **Owner-attributed**: Tests belong to the module they test
- **Isolated**: No external dependencies (database, network, filesystem)
- **Deterministic**: Same input → same output
- **Fast**: No I/O, no waiting, minimal setup

---

## Template

```typescript
// test/{owner}/{feature}.test.ts
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { featureUnderTest } from '../../src/{owner}/{module}';

// Mock dependencies at registry level
vi.mock('../../src/channels/registry.js', () => ({
  normalizeAnyChannelId: vi.fn((id) => `normalized-${id}`),
}));

describe('{Owner} - {Feature}', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('pure logic', () => {
    it('should handle valid input', () => {
      const result = featureUnderTest({ valid: 'input' });
      expect(result).toEqual({ expected: 'output' });
    });

    it('should handle edge cases', () => {
      const result = featureUnderTest({ edge: 'case' });
      expect(result).toBeNull(); // or appropriate assertion
    });
  });

  describe('error handling', () => {
    it('should throw on invalid input', () => {
      expect(() => featureUnderTest(null)).toThrow();
    });
  });
});
```

---

## Migration Checklist

When moving from integration to pure tests:

- [ ] Identify logic owner module
- [ ] Extract pure functions to `{module}.runtime.ts` if needed
- [ ] Mock at registry level (not plugin index)
- [ ] Remove database/network dependencies
- [ ] Add explicit test fixtures
- [ ] Attribute test to owner in test file header

---

## Owner Attribution

Add ownership to test headers:

```typescript
/**
 * @owner {github-username}
 * @module {module-name}
 * @pure
 */
```

---

## Related Commits

- `2681bbd9e7` - Move plugin list formatting to pure tests
- `e2477ff726` - Move node pairing authz to pure coverage
- `7273cae36b` - Move spawn and doctor coverage to owners
- `7e66a8fcfe` - Move plugin uninstall selection to pure tests
- `5ca92b0498` - Move plugin update selection to pure tests

---

*Auto-generated from OpenEvolve Night Cycle analysis*
