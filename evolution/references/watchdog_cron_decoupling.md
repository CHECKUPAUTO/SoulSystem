# Watchdog-Cron Decoupling Pattern

**Date:** 2026-04-13  
**Source:** Night Cycle Reports (00:52, 01:30)  
**Status:** Proposal  
**Priority:** P1 (addresses issue #65576)  
**Bug Reference:** Issue #65576 — Cron silently disables LLM idle watchdog; hung providers block failover  

## Problem

The LLM idle watchdog is gated by cron scheduling state. When cron silently disables (error, misconfiguration, restart), the watchdog stops firing. A hung LLM provider then blocks failover indefinitely because no timeout triggers.

## Pattern: Independent Watchdog Timer

```typescript
class IndependentWatchdog {
  private timer: NodeJS.Timeout | null = null;
  private lastActivity: number = Date.now();
  
  constructor(
    private readonly timeoutMs: number,
    private readonly onTimeout: () => void
  ) {}

  /** Called on any LLM activity — resets the timer */
  recordActivity(): void {
    this.lastActivity = Date.now();
    this.resetTimer();
  }

  /** Start or restart the independent timer */
  private resetTimer(): void {
    if (this.timer) clearTimeout(this.timer);
    this.timer = setTimeout(() => {
      if (Date.now() - this.lastActivity > this.timeoutMs) {
        this.onTimeout();
      }
    }, this.timeoutMs);
  }

  /** Start the watchdog — independent of any scheduler */
  start(): void {
    this.resetTimer();
  }

  /** Stop the watchdog cleanly */
  stop(): void {
    if (this.timer) {
      clearTimeout(this.timer);
      this.timer = null;
    }
  }
}
```

## Key Principles

1. **Watchdog is always independent of cron** — never gate watchdog on cron state
2. **Reset on LLM activity** — any response, token, or heartbeat resets the timer
3. **Fail-safe** — if watchdog can't start, refuse the run (don't silently proceed)
4. **Configurable timeout** — per-run override + global default in gateway config
5. **No shared state with cron** — watchdog timer and cron timer are separate objects

## Configuration Schema

```yaml
agents:
  idleWatchdog:
    defaultTimeoutMs: 300000  # 5 minutes
    minTimeoutMs: 60000      # 1 minute minimum
    # Per-run override via request config:
    # run.config.idleWatchdogTimeoutMs
```

## Related Patterns

- `circuit_breaker_pattern.md` — Circuit breaker for cascading failures
- Agent idle watchdog: commit `7f2814f` — "Honor explicit run timeout for LLM idle watchdog"

## Upstream Tracking

- Issue #65576: Cron silently disables LLM idle watchdog
- Commit `7f2814f`: Honor explicit run timeout for LLM idle watchdog