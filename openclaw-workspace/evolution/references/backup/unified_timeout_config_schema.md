# Unified Timeout Configuration Schema Proposal

**Created:** 2026-04-13 (Night Cycle 00:34)
**Source:** OpenEvolve Night Cycle Report 2026-04-13 00:34
**Status:** Proposal
**Priority:** P2

## Context

Multiple recent fixes address timeout-related issues:
- `7f2814fc4a` — Honor explicit run timeout for LLM idle watchdog
- `ddefce3c18` — Align LLM idle timeout defaults
- Session timeout pure test extraction

Timeouts are currently scattered across config files with inconsistent defaults and no validation.

## Proposal: Unified Timeout Config Object

```typescript
interface TimeoutConfig {
  llm: {
    idleWatchdogMs: number;      // default: 300000 (5min)
    requestTimeoutMs: number;     // default: 120000 (2min)
    retryDelayMs: number;         // default: 1000
    maxRetries: number;           // default: 3
  };
  session: {
    maxDurationMs: number;        // default: 3600000 (1hr)
    idleTimeoutMs: number;        // default: 600000 (10min)
    cleanupIntervalMs: number;    // default: 300000 (5min)
  };
  harness: {
    spawnTimeoutMs: number;       // default: 60000 (1min)
    healthCheckIntervalMs: number; // default: 10000 (10s)
    circuitBreakerResetMs: number; // default: 30000 (30s)
  };
}

// Defensive defaults with validation
function validateTimeoutConfig(config: Partial<TimeoutConfig>): TimeoutConfig {
  const defaults = getDefaultTimeoutConfig();
  const merged = deepMerge(defaults, config);
  // Ensure no timeout is less than minimum
  assertTimeoutBounds(merged);
  return merged;
}
```

### Key Principle

**Defaults must be defensive** — timeout values should err on the side of longer waits rather than premature kills. The LLM idle watchdog fix (`7f2814fc4a`) demonstrated that misaligned defaults cause session kills.

## References

- LLM idle watchdog fix: `7f2814fc4a`
- Timeout alignment: `ddefce3c18`
- Session timeout pure tests: multiple `test:` commits