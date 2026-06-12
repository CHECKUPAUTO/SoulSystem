# Cron Test Infrastructure: Smoke Test Harness

**Priority:** P2 (MEDIUM)
**Source:** Night Cycle 2026-04-13 01:17
**Status:** Proposal

## Problem

The growing cron regression test suite (`tools-invoke-http.cron-regression.test.ts`) has significant test setup boilerplate. Each cron-related test file duplicates mock patterns for gateway, session store, and tools registry. As the suite grows, this duplication increases maintenance burden.

## Proposal

Create a shared `CronTestHarness` utility that encapsulates common setup/teardown patterns.

### Architecture

```typescript
// test/helpers/cron-test-harness.ts

interface CronTestConfig {
  /** Mock gateway responses */
  gatewayMocks?: Partial<GatewayMockConfig>;
  /** Mock session store responses */
  sessionStoreMocks?: Partial<SessionStoreMockConfig>;
  /** Tools registry mock */
  toolsRegistry?: Partial<ToolsRegistryMockConfig>;
  /** Cron schedule override */
  schedule?: string;
  /** Timeout for test assertions */
  assertionTimeout?: number;
}

class CronTestHarness {
  constructor(config: CronTestConfig);
  
  /** Standardized setup: create mocks, register tools, start scheduler */
  async setup(): Promise<void>;
  
  /** Standardized teardown: clear mocks, stop scheduler, cleanup timers */
  async teardown(): Promise<void>;
  
  /** Wait for next cron tick with configurable timeout */
  async waitForTick(): Promise<CronResult>;
  
  /** Assert that a tool was invoked with expected params */
  assertToolInvoked(toolName: string, params?: Record<string, unknown>): void;
  
  /** Assert that no tool was invoked since last tick */
  assertNoToolInvoked(): void;
  
  /** Get all invocations since last tick */
  getInvocations(): ToolInvocation[];
  
  /** Reset invocation history without full teardown */
  reset(): void;
}
```

### Usage Pattern

```typescript
// Before: 50+ lines of setup per test
describe('tools-invoke-http cron', () => {
  let harness: CronTestHarness;
  
  beforeEach(async () => {
    harness = new CronTestHarness({
      gatewayMocks: { baseUrl: 'http://localhost:9999' },
      schedule: '*/5 * * * *',
    });
    await harness.setup();
  });
  
  afterEach(async () => {
    await harness.teardown();
  });
  
  it('invokes HTTP tool on schedule', async () => {
    const result = await harness.waitForTick();
    harness.assertToolInvoked('http_request', { url: 'http://localhost:9999/webhook' });
  });
});
```

### Benefits

- Reduces boilerplate across cron test files (estimated 40-60% reduction)
- Standardized mock patterns prevent subtle differences between test files
- Easier to add new cron-related tests
- Centralized cleanup prevents test leakage
- Aligns with `test_mock_consolidation_guide.md` and `pure_test_migration_pattern.md`

### Related References

- `test_mock_consolidation_guide.md` — Centralized test fixture patterns
- `test_harness_standardization.md` — Test harness standards
- `pure_test_migration_tracker.md` — Ongoing pure test migration tracking
- OpenClaw commit `6883273` — Cron regression harness hardening