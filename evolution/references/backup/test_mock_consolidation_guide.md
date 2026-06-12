# Test Mock Consolidation Guide
**Generated:** 2026-04-11 13:41 UTC  
**Source:** night_cycle_20250411_0748.md  
**Status:** ✅ Documented Best Practice

---

## Problem

Multiple test files independently mock `normalizeChannelId` and other channel utilities, leading to:
- Code duplication
- Inconsistent mock behavior
- Maintenance overhead
- Test fragility

---

## Detected Duplication

From commit analysis, the following mocks appear across multiple test files:

| Mock | Files | Pattern |
|------|-------|---------|
| `normalizeChannelId` | 7+ files | Independent stubbing |
| `message action aliases` | 3+ files | Repeated setup |
| `doctor legacy config` | 2+ files | Duplicate fixtures |

---

## Recommended Solution

### Create Shared Test Fixtures

```typescript
// test/fixtures/channel-mocks.ts
export const createChannelMock = (overrides = {}) => ({
  normalizeChannelId: jest.fn((id) => `normalized-${id}`),
  normalizeAnyChannelId: jest.fn((id) => `any-${id}`),
  ...overrides
});

export const STATIC_CHANNEL_FIXTURES = {
  matrix: { dmAllowFromMode: "nestedOnly", groupModel: "sender" },
  msteams: { dmAllowFromMode: "topOnly", groupModel: "hybrid" },
  telegram: { dmAllowFromMode: "topOnly", groupModel: "sender" },
};

export const mockChannelRegistry = () => {
  jest.mock("../../channels/registry.js", () => ({
    normalizeAnyChannelId: jest.fn((id) => `normalized-${id}`),
    ...createChannelMock()
  }));
};
```

### Usage in Tests

```typescript
// Before: Duplicated in each test file
jest.mock("../../channels/plugins/index.js", () => ({
  normalizeChannelId: jest.fn()
}));

// After: Centralized fixture
import { mockChannelRegistry } from "../fixtures/channel-mocks.js";

beforeEach(() => {
  mockChannelRegistry();
});
```

---

## Benefits

1. **Consistency** - All tests use same mock behavior
2. **Maintainability** - Update once, apply everywhere
3. **Clarity** - Intent is explicit via fixture name
4. **Refactoring Safety** - Changes to registry API caught centrally

---

## Migration Plan

1. Create `test/fixtures/channel-mocks.ts`
2. Migrate 3 highest-usage test files
3. Validate test behavior unchanged
4. Document pattern for new tests
5. Gradually migrate remaining files

---

## Related Commits

- `3edc8d3028` - Mock message action aliases in normalization
- `7a1cc53b18` - Mock message action channel aliases
- `d86377acfd` - Narrow doctor legacy config aliases

---

*Auto-generated from OpenEvolve Night Cycle analysis*
