# Adaptive Idle Timeout Strategy

**Priority:** High (from 0219 report, Improvement 1)  
**Status:** Proposal  
**Created:** 2026-04-13  
**Source:** OpenEvolve Night Cycle 0219

## Problem

The LLM idle watchdog fix (`7f2814fc4a`) honors explicit timeouts but lacks adaptive behavior. Known slow models may need longer timeouts, and a one-size-fits-all approach causes either premature termination or unnecessary waiting.

## Proposal

```typescript
// adaptive-idle-timeout.ts

const IDLE_TIMEOUT_BASE = 30_000; // 30s base
const IDLE_TIMEOUT_MULTIPLIER = 1.5; // 50% grace period
const KNOWN_SLOW_MODELS = new Set([
  'o3', 'o3-mini', 'gemini-2.5-pro', // models with long reasoning
]);

const computeEffectiveTimeout = (explicit?: number, model?: string) => {
  if (explicit) return explicit;
  // Known slow models get longer timeouts
  const modelFactor = model && KNOWN_SLOW_MODELS.has(model) ? 2.0 : 1.0;
  return IDLE_TIMEOUT_BASE * IDLE_TIMEOUT_MULTIPLIER * modelFactor;
};
```

## Key Design Decisions

1. **Explicit timeouts always win** — No override of user-specified values
2. **Model-aware defaults** — Slow reasoning models get 2x timeout
3. **Grace period multiplier** — 50% buffer above base for network variance
4. **Configurable constants** — Easy to tune without code changes

## Benefits

- Prevents premature termination of long-running LLM calls
- Adapts to model characteristics automatically
- Maintains backward compatibility with explicit timeout overrides

## Related References

- `unified_timeout_config_schema.md` — Broader timeout configuration proposal
- `watchdog_cron_decoupling.md` — Independent watchdog timer pattern

## Upstream Reference

- Commit `7f2814fc4a` — agents: honor explicit run timeout for LLM idle watchdog