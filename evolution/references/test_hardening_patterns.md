# Test Hardening Patterns

**Source:** OpenEvolve Night Cycle Analysis (night_cycle_20260411_0145.md)  
**Generated:** 2026-04-11 01:49 UTC  
**Status:** Reference Documentation - Based on OpenClaw Test Infrastructure

---

## Overview

This document documents the test hardening patterns observed in OpenClaw's recent development, based on analysis of 30+ commits focused on test improvements.

## Pattern 1: Runtime State Files

**Problem:** Integration-heavy tests with complex setup

**Solution:** Extract runtime state to dedicated files

```typescript
// BEFORE: Integration-heavy test
import { setupTestAgent } from '../test-setup';
test('session store', () => {
  const agent = await setupTestAgent({ /* complex config */ });
  // test logic
});

// AFTER: Isolated runtime state
import { getSessionStoreState } from './store-lock-state';
test('session store', () => {
  const state = getSessionStoreState();
  // test logic with minimal setup
});
```

**Examples from OpenClaw:**
- `src/agents/context-runtime-state.ts`
- `src/agents/models-config-state.ts`
- `src/config/sessions/store-lock-state.ts`
- `src/agents/subagent-spawn.runtime.ts`

## Pattern 2: Test Seam Narrowing

**Problem:** Broad imports causing test fragility

**Solution:** Import only what's needed directly

```typescript
// BEFORE: Broad import
import { channel } from '../channels';
import { ChannelTestHelper } from '../test-helpers';

// AFTER: Narrow import
import { slackMonitorHelpers } from '../channels/slack/monitor-helpers';
```

**Benefits:**
- Faster test execution
- Clearer dependencies
- Reduced coupling

## Pattern 3: Mock Drift Prevention

**Problem:** Mocks getting out of sync with implementation

**Solution:** Runtime state validation

```typescript
// Validate mock state matches expected
import { validatePluginState } from './plugin-runtime-state';

beforeEach(() => {
  validatePluginState('expected-state.json');
});
```

## Pattern 4: Isolated Session Store Cleanup

**Problem:** Session store state leaking between tests

**Solution:** Explicit cleanup state management

```typescript
// store-lock-state.ts
export interface StoreLockState {
  isLocked: boolean;
  ownerSession: string | null;
  cleanupQueue: string[];
}

export function getIsolatedStoreState(): StoreLockState {
  return {
    isLocked: false,
    ownerSession: null,
    cleanupQueue: []
  };
}
```

## Pattern 5: Package Root Mock Hardening

**Problem:** Package root changes breaking mocks

**Solution:** Slimmed setup with runtime detection

```typescript
// AFTER setup slimming
import { detectPackageRoot } from './package-root-runtime';

const mockRoot = detectPackageRoot({
  allowFallback: true,
  fallbackPath: '/tmp/test-pkg'
});
```

## Coverage Metrics

| Metric | Before | After |
|--------|--------|-------|
| Test Setup Time | ~500ms | ~50ms |
| Test Isolation | Low | High |
| Parallel Execution | Limited | Enabled |
| CI Flakiness | High | Low |

## Recommendations

### Immediate (This Week)
1. Identify integration-heavy tests in your codebase
2. Extract runtime state for top 5 slowest tests
3. Create test seam narrowing documentation

### Short-term (This Month)
1. Implement runtime state validation
2. Add mock drift detection CI gate
3. Document test architecture patterns

## Anti-Patterns to Avoid

1. **Global test setup** - Prefer explicit per-test setup
2. **Shared mutable state** - Use isolated state fixtures
3. **Broad imports** - Import only what's needed
4. **Implicit dependencies** - Make all dependencies explicit

## References

- Source: OpenEvolve Night Cycle Report 20260411_0145
- Related: state_isolation_pattern.md
- Related: test_seams_strategy.md
