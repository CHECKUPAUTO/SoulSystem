# iMessage Monitor Circuit Breaker

**Source:** OpenEvolve Night Cycle 2026-04-12 23:33  
**Priority:** High (reliability)  
**Impact:** macOS user stability

## Problem

Three consecutive commits fixing iMessage monitor startup and retry logic indicate a real reliability pain point. The `watch.subscribe` failures on startup suggest race conditions between the iMessage monitoring service and system-level launchd/socket activation.

## Pattern: Circuit Breaker for iMessage Monitor

Apply the circuit breaker pattern (documented in `circuit_breaker_pattern.md`) to the iMessage monitor:

```typescript
import { CircuitBreaker } from './circuit-breaker';

const iMessageMonitor = new CircuitBreaker({
  maxRetries: 5,
  baseDelayMs: 1000,
  maxDelayMs: 30000,
  resetTimeoutMs: 60000,
  onOpen: () => logger.warn('iMessage monitor circuit open'),
  onHalfOpen: () => logger.info('iMessage monitor attempting recovery'),
});

async function startMonitor() {
  return iMessageMonitor.execute(() => watch.subscribe());
}
```

### Current Fix Commits

- `35a784c165` fix(imessage): retry watch.subscribe startup failures
- `ea71a59127` fix(imessage): repair monitor retry type checks
- `fa87c6334a` fix(imessage): align monitor retry types

These fixes add ad-hoc retry logic. A circuit breaker provides:
- **Exponential backoff** instead of fixed retry intervals
- **State tracking** (closed → open → half-open) for observability
- **Automatic recovery** after cooldown period
- **Graceful degradation** when the monitor can't start

### Related References

- `circuit_breaker_pattern.md` — generic circuit breaker pattern (ported from VisionClaw)
- `visionclaw_security_reediation_guide.md` — VisionClaw resilience patterns