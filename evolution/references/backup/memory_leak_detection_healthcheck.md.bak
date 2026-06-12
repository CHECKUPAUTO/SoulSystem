# Memory Leak Detection in Healthcheck

**Priority:** P2 (MEDIUM)
**Source:** Night Cycle 2026-04-13 01:15
**Status:** Proposal

## Problem

Three separate memory leak fixes in one week (#64258, #64156, #64505) suggest systematic leak patterns in long-running daemon processes. Currently, there's no early warning system for memory growth before OOM conditions occur.

## Proposal

Add periodic `process.memoryUsage()` delta tracking to the `openclaw-doctor` skill's heartbeat checks.

### Implementation

Add to healthcheck skill's `HEARTBEAT.md` or scheduled cron:

```bash
# In healthcheck heartbeat
MEM_JSON=$(node -e "
  const m = process.memoryUsage();
  console.log(JSON.stringify({
    rss: m.rss,
    heapTotal: m.heapTotal,
    heapUsed: m.heapUsed,
    external: m.external,
    timestamp: Date.now()
  }));
")

# Store in heartbeat-state.json under memoryDeltas
# Alert if heapUsed grows > 50MB over 24h without corresponding traffic increase
```

### Tracking State

```json
{
  "memoryBaseline": {
    "heapUsed": 150000000,
    "rss": 250000000,
    "timestamp": 1713369600000
  },
  "memoryDeltas": [
    { "delta": 5000000, "hoursElapsed": 6, "timestamp": 1713391200000 },
    { "delta": 12000000, "hoursElapsed": 12, "timestamp": 1713412800000 }
  ],
  "alertThreshold": "50MB/24h"
}
```

### Alert Conditions

- **Heap growth > 50MB/24h** — Warning: potential leak
- **Heap growth > 100MB/24h** — Critical: likely leak
- **RSS > 2x heapTotal** — Warning: external memory accumulation

## Benefits

- Early warning before OOM kills the gateway
- Data-driven leak detection (not just "feels slow")
- Correlation with deployment/commit timestamps for regression tracking
- Works alongside existing healthcheck infrastructure

## Related References

- `openclaw-doctor` skill — existing health monitoring
- `circuit_breaker_pattern.md` — resilience patterns
- OpenClaw issues #64258, #64156, #64505 — recent leak fixes