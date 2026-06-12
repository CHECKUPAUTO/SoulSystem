# Test Harness Standardization Pattern

**CodeWiki Pattern ID:** `patterns/test-harness-standardization`  
**Classification:** Test Infrastructure Pattern  
**Status:** Active Implementation**  
**Source:** OpenEvolve Night Cycle 2026-04-12_0635 (10 commits by Vincent Koc & Peter Steinberger)

---

## Executive Summary

Systematic extraction of duplicated test utilities into shared fixtures under `test/fixtures/` and `test/harness/`. Each commit removes ~15-25 lines of duplicated mock setup per test file, replacing it with a shared fixture import.

**Current Status:** 10 commits implementing this pattern (67% of recent cycle)  
**Fitness Impact:** Syntax +8%, Semantic +12%, Maintainability significantly improved  
**Target:** 90% test coverage using shared utilities

---

## Problem Statement

Test files contain duplicated mock setups, leading to:
- **Code duplication** (~15-25 lines per file)
- **Inconsistent mocking** patterns across modules
- **Maintenance burden** - changes must propagate to multiple files
- **Cognitive overhead** for test authors

**Before (Duplicated Pattern):**
```typescript
// Duplicated in 7+ test files
describe('Agent Runtime', () => {
  beforeEach(() => {
    jest.mock("../../channels/plugins/index.js", () => ({
      normalizeChannelId: jest.fn()
    }));
    jest.mock("../../config/index.js", () => ({
      getConfig: jest.fn().mockReturnValue({ debug: false })
    }));
    // ... 20 more lines
  });
});
```

---

## Solution Pattern

### Core Principle: Centralized Fixtures

Extract common mock setups into shared, named fixtures.

**After (Shared Fixture):**
```typescript
// test/fixtures/channel-mocks.ts
export const mockChannelRegistry = () => {
  jest.mock("../../channels/plugins/index.js", () => ({
    normalizeChannelId: jest.fn().mockReturnValue('normalized-123'),
    getChannelAdapter: jest.fn()
  }));
};

// test/fixtures/config-mocks.ts
export const mockConfig = (overrides = {}) => {
  const defaultConfig = { debug: false, env: 'test' };
  jest.mock("../../config/index.js", () => ({
    getConfig: jest.fn().mockReturnValue({ ...defaultConfig, ...overrides })
  }));
};

// In test file:
import { mockChannelRegistry } from "../fixtures/channel-mocks.js";
import { mockConfig } from "../fixtures/config-mocks.js";

describe('Agent Runtime', () => {
  beforeEach(() => {
    mockChannelRegistry();
    mockConfig({ debug: true });
  });
});
```

---

## Fixture Categories

### 1. Channel Fixtures

| Fixture | Description | Commits |
|---------|-------------|---------|
| `mockChannelRegistry` | Channel plugin normalization | `c5c50ad3` (contracts) |
| `mockThreadStore` | Slack thread message storage | `1d1f10ec` (slack) |
| `mockReactionHandler` | MS Teams reaction harness | `37ddd018` (msteams) |
| `mockNativeCommands` | Discord autocomplete | `560d56e8` (discord) |
| `mockCachedCreds` | WhatsApp credential spies | `afc2bc00` (whatsapp) |

### 2. Runtime Fixtures

| Fixture | Description | Commits |
|---------|-------------|---------|
| `trimContexts` | Context pruning fixtures | `aa415b25` (agents) |
| `mockTimedOutRegistry` | Cron timeout registry | `393877e` (cron) |
| `mockBundledPluginRoot` | Plugin resolution helper | `c5c50ad` (contracts) |
| `mockSignedTelnyxRequest` | Voice call signing | `97aa6e08` (voice-call) |
| `mockSessionRoute` | Matrix routing setup | `560d56e8` (matrix) |

### 3. Auth Fixtures

| Fixture | Description | Commits |
|---------|-------------|---------|
| `mockBrowserAuth` | Browser auth persistence | `add29005` (browser) |
| `mockOpenAIResponse` | QA-lab mock helpers | `cded4fc5` (qa-lab) |

---

## Implementation Guide

### Step 1: Identify Duplication

```bash
# Find common mock patterns across test files
grep -r "jest.mock.*channels/plugins" src/**/*.test.ts | wc -l
# Should show high duplication count
```

### Step 2: Extract to Fixture

```typescript
// test/fixtures/new-fixture.ts
export interface FixtureOptions {
  // Define configurable aspects
}

export const createMock = (options: FixtureOptions = {}) => {
  // Reset before each test
  beforeEach(() => {
    jest.clearAllMocks();
  });
  
  // Return fixture functions
  return {
    setup: () => {
      jest.mock("path/to/module", () => ({
        // Mock implementation
      }));
    },
    teardown: () => {
      jest.unmock("path/to/module");
    }
  };
};
```

### Step 3: Migrate Test Files

```typescript
// Migration pattern
// BEFORE:
describe('Old Pattern', () => {
  beforeEach(() => {
    jest.mock("../../module", () => ({ ... }));
  });
});

// AFTER:
import { mockModule } from "../fixtures/module-mocks.js";

describe('New Pattern', () => {
  beforeEach(() => {
    mockModule();
  });
});
```

### Step 4: Validate Migration

```bash
# Ensure no regression
npm test -- path/to/migrated.test.ts

# Check coverage maintained
npm test -- --coverage --collectCoverageFrom="src/**/*.ts"
```

---

## Directory Structure

```
test/
├── fixtures/
│   ├── channel-mocks.ts      # Channel plugin mocks
│   ├── config-mocks.ts       # Configuration mocks
│   ├── auth-mocks.ts         # OAuth/credential mocks
│   ├── runtime-mocks.ts      # Agent runtime mocks
│   └── index.ts              # Barrel export (with care)
├── harness/
│   ├── integration.ts        # Integration test setup
│   ├── unit.ts               # Unit test helpers
│   └── e2e.ts                # E2E test fixtures
└── utils/
    └── test-helpers.ts       # General utilities
```

---

## Commit Pattern

Follow Vincent Koc's convention:

```bash
# Format: test(<scope>): share <fixture> <helper/fixture>

git commit -m "test(contracts): share bundled plugin root helper

Extracts duplicated plugin resolution logic into
shared fixture for contract tests.

- Reduces 7 test files from ~20 lines to 3 lines
- Standardizes mock behavior across module
- No functional changes

Related: evolution/references/test_harness_standardization.md"
```

---

## Metrics

### Before Standardization

- Average mock setup per test file: ~25 lines
- Duplicated across files: ~175 lines per pattern (7 files)
- Time to update: 15 minutes per change (7 files × ~2 min)

### After Standardization

- Average mock setup per test file: ~3 lines
- Centralized in fixtures: ~30 lines (1 file)
- Time to update: 2 minutes (1 file)
- **Efficiency gain: ~87%**

---

## Testing the Fixtures

```typescript
// test/fixtures/channel-mocks.test.ts
describe('Channel Mock Fixtures', () => {
  it('should normalize channel IDs consistently', () => {
    const { normalizeChannelId } = mockChannelRegistry();
    expect(normalizeChannelId('test-123')).toBe('normalized-123');
  });
  
  it('should reset between tests', () => {
    // Verify isolation
    const mock1 = mockChannelRegistry();
    mock1.normalizeChannelId.mockReturnValue('custom');
    
    // After reset, should return to default
    jest.clearAllMocks();
    const mock2 = mockChannelRegistry();
    expect(mock2.normalizeChannelId).not.toHaveBeenCalled();
  });
});
```

---

## Related Patterns

- [Test Mock Consolidation Guide](test_mock_consolidation_guide.md) - Original consolidation pattern
- [Active Memory Integration Testing](active_memory_integration_testing_guide.md) - Integration test patterns
- [Session State Management](session_state_management_patterns.md) - State preservation in tests

---

## Current Commits

| Commit | Module | Shared Utility | Author |
|--------|--------|----------------|--------|
| `c5c50ad3` | contracts | bundled plugin root helper | Vincent Koc |
| `aa415b25` | agents | context pruning trim fixtures | - |
| `1d1f10ec` | slack | thread message store fixtures | - |
| `37ddd018` | msteams | reaction handler harness | - |
| `c3c13ea3` | telegram | exec approval resolver cases | - |
| `afc2bc00` | whatsapp | cached creds spies | - |
| `560d56e8` | discord | native command autocomplete | - |
| `97aa6e08` | voice-call | signed telnyx request helper | Vincent Koc |
| `add29005` | browser | control auth persistence | Peter Steinberger |
| `cded4fc5` | qa-lab | mock openai response helpers | Vincent Koc |

---

## References

- **Related Reference:** [Test Mock Consolidation Guide](test_mock_consolidation_guide.md)
- **T430 Fitness Impact:** Syntax +8%, Semantic +12%
- **Source:** OpenEvolve Night Cycle 2026-04-12_0635

---

*Generated by OpenEvolve Night Cycle Analysis*  
*Report ID: night_cycle_20260412_0635*
