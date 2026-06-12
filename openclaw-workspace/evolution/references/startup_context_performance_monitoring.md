# Startup Context Performance Monitoring

**Source:** OpenEvolve Night Cycle Report 2026-04-12 03:15  
**Priority:** P2  
**Related Commit:** 4d0f555 (fix: preload startup memory for bare session resets)

---

## Background

Commit 4d0f555 introduced `startup-context.ts` (136 lines) with comprehensive test coverage (102 lines). This module preloads memory for bare session resets, but may have performance implications with large memory files.

---

## Performance Monitoring Strategy

### 1. Instrumentation Points

```typescript
// src/auto-reply/reply/startup-context.ts
import { metrics } from '../../metrics';

export interface StartupContext {
  memory: MemoryEntry[];
  templates: AgentTemplate[];
  runtime: RuntimeConfig;
}

export async function preloadStartupContext(
  options?: PreloadOptions
): Promise<StartupContext> {
  const startTime = performance.now();
  const memoryStart = process.memoryUsage();

  try {
    // ... existing preload logic ...
    const result = await loadStartupContext();

    // Record metrics
    const duration = performance.now() - startTime;
    const memoryUsed = process.memoryUsage().heapUsed - memoryStart.heapUsed;

    metrics.record('startup_context_preload_ms', duration);
    metrics.record('startup_context_memory_bytes', memoryUsed);
    metrics.record('startup_context_memory_entries', result.memory.length);

    // Log slow preloads
    if (duration > SLOW_PRELOAD_THRESHOLD_MS) {
      logger.warn('Slow startup context preload detected', {
        duration,
        entries: result.memory.length,
        memoryBytes: memoryUsed,
      });
    }

    return result;
  } catch (error) {
    metrics.increment('startup_context_errors');
    throw error;
  }
}

const SLOW_PRELOAD_THRESHOLD_MS = 500; // Configurable
```

### 2. Metric Definitions

```typescript
// src/metrics/definitions.ts
export const STARTUP_CONTEXT_METRICS = {
  // Duration histogram
  'startup_context_preload_ms': {
    type: 'histogram',
    description: 'Time to preload startup context',
    buckets: [10, 50, 100, 250, 500, 1000, 2500],
    labels: ['status'],
  },

  // Memory usage gauge
  'startup_context_memory_bytes': {
    type: 'gauge',
    description: 'Heap memory used during preload',
    labels: ['phase'],
  },

  // Entry count
  'startup_context_memory_entries': {
    type: 'histogram',
    description: 'Number of memory entries loaded',
    buckets: [10, 50, 100, 500, 1000, 5000],
  },

  // Error counter
  'startup_context_errors': {
    type: 'counter',
    description: 'Total preload errors',
    labels: ['error_type'],
  },

  // Cache hit rate
  'startup_context_cache_hits': {
    type: 'counter',
    description: 'Cached context hits',
  },
} as const;
```

### 3. Alerting Rules

```yaml
# monitoring/alerts.yml
groups:
  - name: startup_context
    rules:
      - alert: SlowStartupContextPreload
        expr: |
          histogram_quantile(0.95, 
            rate(startup_context_preload_ms_bucket[5m])
          ) > 1000
        for: 2m
        labels:
          severity: warning
        annotations:
          summary: "Slow startup context preload detected"
          description: "95th percentile preload time > 1s"

      - alert: StartupContextHighMemory
        expr: startup_context_memory_bytes > 100 * 1024 * 1024
        for: 1m
        labels:
          severity: warning
        annotations:
          summary: "High memory usage during startup"
          description: "Startup context using >100MB"

      - alert: StartupContextErrors
        expr: rate(startup_context_errors[5m]) > 0.1
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "Startup context preload failures"
```

### 4. Performance Dashboard

```json
{
  "dashboard": {
    "title": "Startup Context Performance",
    "panels": [
      {
        "title": "Preload Duration (p50/p95/p99)",
        "targets": [
          {
            "expr": "histogram_quantile(0.50, rate(startup_context_preload_ms_bucket[5m]))",
            "legend": "p50"
          },
          {
            "expr": "histogram_quantile(0.95, rate(startup_context_preload_ms_bucket[5m]))",
            "legend": "p95"
          }
        ]
      },
      {
        "title": "Memory Usage",
        "targets": [
          {
            "expr": "startup_context_memory_bytes",
            "legend": "Heap Used"
          }
        ]
      },
      {
        "title": "Entry Count Distribution",
        "targets": [
          {
            "expr": "startup_context_memory_entries",
            "legend": "Entries"
          }
        ]
      }
    ]
  }
}
```

---

## Optimization Recommendations

### 1. Lazy Loading
```typescript
// Instead of loading all memory upfront
export async function preloadStartupContextLazy(): Promise<StartupContext> {
  return {
    memory: createLazyLoader(() => loadMemoryEntries()),
    templates: createLazyLoader(() => loadTemplates()),
    runtime: await loadRuntimeConfig(), // Always eager
  };
}
```

### 2. Pagination for Large Memory
```typescript
export async function preloadStartupContextPaginated(
  options: { limit?: number; cursor?: string } = {}
): Promise<StartupContext> {
  const { entries, nextCursor } = await loadMemoryEntriesPaginated({
    limit: options.limit ?? 100,
    cursor: options.cursor,
  });

  return {
    memory: entries,
    pagination: { hasMore: !!nextCursor, cursor: nextCursor },
    // ...
  };
}
```

### 3. Caching Layer
```typescript
const contextCache = new Map<string, CachedContext>();

export async function preloadStartupContextCached(
  cacheKey: string
): Promise<StartupContext> {
  const cached = contextCache.get(cacheKey);
  if (cached && !isExpired(cached)) {
    metrics.increment('startup_context_cache_hits');
    return cached.data;
  }

  const fresh = await preloadStartupContext();
  contextCache.set(cacheKey, {
    data: fresh,
    timestamp: Date.now(),
  });

  return fresh;
}
```

---

## Benchmarking

```typescript
// test/performance/startup-context.bench.ts
import { bench, describe } from 'vitest';
import { preloadStartupContext } from '../../src/auto-reply/reply/startup-context';

describe('Startup Context Performance', () => {
  bench('preload with 10 entries', async () => {
    await preloadStartupContext({ mockEntries: 10 });
  });

  bench('preload with 100 entries', async () => {
    await preloadStartupContext({ mockEntries: 100 });
  });

  bench('preload with 1000 entries', async () => {
    await preloadStartupContext({ mockEntries: 1000 });
  });
});
```

---

## References

- Source Report: `night_cycle_20260412_0315.md`
- Related Commit: 4d0f555
- Related Pattern: `session_state_management_patterns.md`
