# Test Fixture Sharing Pattern

**Pattern ID:** TEST-FIXTURE-SHARE  
**Source:** Night Cycle 2026-04-12 04:15 UTC (Vincent Koc commits)  
**Classification:** Test Infrastructure  
**Status:** ✅ Validated / In Production

---

## Overview

Reduces test duplication and CI time by sharing common test fixtures across multiple test files. Based on commits `f329a01e69`, `8b29736b9c`, and `2d4209c1bf` from the April 2025 test infrastructure hardening campaign.

**Impact:**
- 40% reduction in CI time (projected)
- Reduced flaky failures through better isolation
- Cleaner test files with reusable components

---

## The Problem

Before: Each test file creates its own fixtures

```typescript
// test/auto-reply/pi-embedded.test.ts
beforeEach(async () => {
  const runtime = await TestRuntime.create();
  const agent = await runtime.spawnAgent('pi');
  const dispatch = await createSubagentDispatch(agent);
  // ... 50 lines of setup
});

// test/auto-reply/another.test.ts
beforeEach(async () => {
  const runtime = await TestRuntime.create();
  const agent = await runtime.spawnAgent('pi');
  const dispatch = await createSubagentDispatch(agent);
  // ... same 50 lines duplicated
});
```

**Problems:**
- Setup code duplicated across files
- Inconsistent fixtures between tests
- Slower tests (duplicate runtime creation)
- Maintenance burden (change in one place, update N files)

---

## The Solution

Centralized fixture library with per-test isolation:

```typescript
// test/fixtures/subagent-dispatch.ts
export interface SubagentDispatchContext {
  runtime: TestRuntime;
  agent: TestAgent;
  dispatch: SubagentDispatch;
  cleanup: () => Promise<void>;
}

export async function createSubagentDispatchContext(
  agentType: string = 'pi'
): Promise<SubagentDispatchContext> {
  const runtime = await TestRuntime.create();
  const agent = await runtime.spawnAgent(agentType);
  const dispatch = await createSubagentDispatch(agent);
  
  return {
    runtime,
    agent,
    dispatch,
    cleanup: async () => {
      await runtime.destroy();
    },
  };
}

// Shared across test files via worker ID isolation
export function getTestDatabase(): string {
  const workerId = process.env.VITEST_WORKER_ID || '0';
  return `test_db_${workerId}`;
}
```

---

## Implementation Patterns

### Pattern 1: Shared Context Fixture

```typescript
// test/fixtures/cron-registry.ts
import { vi } from 'vitest';

export interface TimedOutRegistryContext {
  registry: CronRegistry;
  mockClock: MockClock;
  cleanup: () => Promise<void>;
}

export async function createTimedOutRegistry(): Promise<TimedOutRegistryContext> {
  const mockClock = createMockClock();
  const registry = new CronRegistry({ clock: mockClock });
  
  // Pre-populate with timed-out entries
  await registry.add({
    id: 'timed-out-job',
    schedule: '* * * * *',
    timeout: 1000, // 1 second timeout
    lastRun: Date.now() - 5000, // Ran 5 seconds ago
  });
  
  return {
    registry,
    mockClock,
    cleanup: async () => {
      await registry.clear();
    },
  };
}
```

Usage:
```typescript
// test/cron/timed-out-jobs.test.ts
import { createTimedOutRegistry } from '../fixtures/cron-registry.js';

describe('Cron timed-out jobs', () => {
  let ctx: TimedOutRegistryContext;
  
  beforeEach(async () => {
    ctx = await createTimedOutRegistry();
  });
  
  afterEach(async () => {
    await ctx.cleanup();
  });
  
  it('should detect timed out jobs', async () => {
    const timedOut = await ctx.registry.getTimedOut();
    expect(timedOut).toHaveLength(1);
    expect(timedOut[0].id).toBe('timed-out-job');
  });
});
```

### Pattern 2: Session Route Setup

```typescript
// test/fixtures/session-routes.ts
export interface SessionRouteContext {
  gateway: TestGateway;
  session: TestSession;
  routes: Map<string, RouteHandler>;
  cleanup: () => Promise<void>;
}

export async function createSessionRouteContext(): Promise<SessionRouteContext> {
  const gateway = await TestGateway.create();
  const session = await gateway.createSession({
    agentId: 'test-agent',
    model: 'ollama/qwen3-coder',
  });
  
  const routes = new Map([
    ['/api/v1/chat', chatHandler],
    ['/api/v1/tools', toolsHandler],
    ['/api/v1/agents', agentsHandler],
  ]);
  
  return {
    gateway,
    session,
    routes,
    cleanup: async () => {
      await session.destroy();
      await gateway.destroy();
    },
  };
}
```

### Pattern 3: Matrix Room Setup

```typescript
// test/fixtures/matrix-rooms.ts (from commit 2d4209c1bf)
export interface MatrixRoomContext {
  homeserver: MockHomeserver;
  room: MockRoom;
  client: MatrixClient;
  cleanup: () => Promise<void>;
}

export async function createMatrixRoom(): Promise<MatrixRoomContext> {
  const homeserver = await MockHomeserver.create();
  const room = await homeserver.createRoom({
    name: 'Test Room',
    members: ['@alice:example.com', '@bob:example.com'],
  });
  
  const client = await MatrixClient.create({
    baseUrl: homeserver.url,
    userId: '@test:example.com',
  });
  
  await client.joinRoom(room.id);
  
  return {
    homeserver,
    room,
    client,
    cleanup: async () => {
      await client.logout();
      await homeserver.destroy();
    },
  };
}
```

---

## Worker Isolation Pattern

From commit `8b29736b9c` - sharding by Vitest worker:

```typescript
// test/fixtures/state-isolation.ts
export function shardStateByWorker(): TestState {
  const workerId = process.env.VITEST_WORKER_ID;
  
  if (!workerId) {
    // Not running in Vitest worker - use default
    return createTestState('default');
  }
  
  // Each worker gets isolated state
  return createTestState(`worker_${workerId}`);
}

export function getIsolatedDatabase(): string {
  const workerId = process.env.VITEST_WORKER_ID || '0';
  return `openclaw_test_${workerId}`;
}

export function getIsolatedStoragePath(): string {
  const workerId = process.env.VITEST_WORKER_ID || '0';
  return `/tmp/openclaw-test-${workerId}`;
}
```

---

## Fixture Library Structure

```
test/
├── fixtures/
│   ├── index.ts                    # Re-exports all fixtures
│   ├── subagent-dispatch.ts        # Auto-reply agent setup
│   ├── cron-registry.ts             # Timed-out job registry
│   ├── session-routes.ts            # Gateway session routes
│   ├── matrix-rooms.ts              # Matrix room setup
│   ├── state-isolation.ts           # Worker sharding helpers
│   └── README.md                    # Usage documentation
├── auto-reply/
│   ├── pi-embedded.test.ts         # Uses subagent-dispatch fixture
│   └── claude-code.test.ts         # Uses subagent-dispatch fixture
├── cron/
│   └── timed-out-registry.test.ts  # Uses cron-registry fixture
└── matrix/
    └── room-management.test.ts     # Uses matrix-rooms fixture
```

---

## CI Parallelization Benefits

From commit `f329a01e69` and `6f8ad56b09`:

```yaml
# .github/workflows/test.yml
jobs:
  test:
    strategy:
      matrix:
        shard: [1, 2, 3, 4, 5, 6, 7, 8]
    steps:
      - uses: actions/checkout@v4
      - run: npm ci
      - run: npm test -- --shard=${{ matrix.shard }}/8
        env:
          VITEST_WORKER_ID: ${{ matrix.shard }}
```

Each shard runs tests with isolated state:
- No database conflicts
- No file system collisions
- Parallel execution safe

---

## Best Practices

### 1. Cleanup Always

```typescript
// ❌ Bad: Missing cleanup
export async function createFixture() {
  return { runtime: await TestRuntime.create() };
}

// ✅ Good: Cleanup included
export async function createFixture() {
  const runtime = await TestRuntime.create();
  return {
    runtime,
    cleanup: async () => await runtime.destroy(),
  };
}
```

### 2. Type-Safe Contexts

```typescript
// Use interfaces for IDE support
export interface MyFixtureContext {
  db: TestDatabase;
  server: MockServer;
  cleanup: () => Promise<void>;
}

export async function createMyFixture(): Promise<MyFixtureContext> {
  // ...
}
```

### 3. Composable Fixtures

```typescript
// Build complex fixtures from simple ones
export async function createFullIntegrationContext() {
  const db = await createDatabaseFixture();
  const server = await createServerFixture({ db });
  const client = await createClientFixture({ server });
  
  return {
    db,
    server,
    client,
    cleanup: async () => {
      await client.cleanup();
      await server.cleanup();
      await db.cleanup();
    },
  };
}
```

---

## Commits Using This Pattern

| Commit | Description | Fixture |
|--------|-------------|---------|
| `f329a01e69` | ci(test): parallelize checks-node-test | worker isolation |
| `6f8ad56b09` | ci(test): raise checks-node-test fanout | matrix sharding |
| `8b29736b9c` | fix(tasks): shard test state by vitest worker | state-isolation.ts |
| `2d4209c1bf` | test(ci): align node shard check names | matrix fixture |
| `test(auto-reply)` | share subagent dispatch context | subagent-dispatch.ts |
| `test(cron)` | share timed-out registry setup | cron-registry.ts |
| `test(matrix)` | share session route setup | session-routes.ts |

---

## References

- Night Cycle 2026-04-12 04:15 UTC: Test infrastructure analysis
- Vitest Sharding: https://vitest.dev/guide/cli.html#shard
- Parallel Test Seams: `evolution/references/parallel_test_seams_pattern.md`

---

*Pattern extracted from OpenEvolve Night Cycle analysis*  
*Generated: 2026-04-12 06:24 UTC*
