# Pure Test Mock Factory

**Priority:** Low (from 0219 report, Improvement 3)  
**Status:** Proposal  
**Created:** 2026-04-13  
**Source:** OpenEvolve Night Cycle 0219

## Problem

19 test commits each create their own mocks for channel plugins, directive checks, and other common dependencies. This leads to duplication and inconsistency.

## Proposal

```typescript
// test/helpers/mock-factory.ts
export const createMockChannelPlugin = (overrides = {}) => ({
  lookup: vi.fn().mockResolvedValue(overrides),
  send: vi.fn().mockResolvedValue({ ok: true }),
  // ... common mock interface
});

export const createMockDirective = (type: string, overrides = {}) => ({
  type,
  resolve: vi.fn().mockReturnValue(overrides),
  validate: vi.fn().mockReturnValue(true),
});

export const createMockSession = (overrides = {}) => ({
  id: 'test-session',
  target: 'test-target',
  ...overrides,
});
```

## Benefits

- Reduces duplication across 19+ pure test migration commits
- Ensures mock consistency across test files
- Single source of truth for mock interfaces
- Easier to update when interfaces change

## Related References

- `pure_test_coverage_map.md` — Coverage tracking
- `test_ownership_map.md` — Test ownership mapping
- `test_purity_metrics.md` — CI metrics for test purity