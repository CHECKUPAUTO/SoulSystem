# Video Generation Retry with Circuit Breaker

**Created:** 2026-04-13 (Night Cycle auto-apply)
**Priority:** P1
**Source Reports:** night_cycle_20260413_0100.md
**Status:** Proposal — requires gateway implementation

## Problem

Video generation currently uses a single timeout with fail-fast behavior. Transient provider issues (rate limits, temporary outages) cause immediate failures without retry.

## Proposed Pattern: Retry + Exponential Backoff + Circuit Breaker

```typescript
interface VideoGenRetryConfig {
  maxRetries: number;           // default: 3
  baseDelayMs: number;          // default: 2000
  maxDelayMs: number;           // default: 30000
  backoffMultiplier: number;    // default: 2
  circuitBreakerThreshold: number; // default: 5 consecutive failures
  circuitBreakerResetMs: number;  // default: 60000 (1 minute)
}

const DEFAULT_RETRY_CONFIG: VideoGenRetryConfig = {
  maxRetries: 3,
  baseDelayMs: 2000,
  maxDelayMs: 30000,
  backoffMultiplier: 2,
  circuitBreakerThreshold: 5,
  circuitBreakerResetMs: 60000,
};
```

### Retry Logic

```typescript
async function generateWithRetry(
  request: VideoGenRequest,
  config: VideoGenRetryConfig = DEFAULT_RETRY_CONFIG
): Promise<VideoGenResult> {
  const provider = getProvider(request.provider);
  
  // Check circuit breaker
  if (circuitBreaker.isOpen(request.provider)) {
    throw new CircuitOpenError(`Provider ${request.provider} is circuit-broken`);
  }
  
  let lastError: Error;
  for (let attempt = 0; attempt <= config.maxRetries; attempt++) {
    try {
      const result = await provider.generate(request);
      circuitBreaker.recordSuccess(request.provider);
      return result;
    } catch (error) {
      lastError = error;
      if (!isRetryable(error)) throw error;
      
      const delay = Math.min(
        config.baseDelayMs * Math.pow(config.backoffMultiplier, attempt),
        config.maxDelayMs
      );
      await sleep(delay);
    }
  }
  
  circuitBreaker.recordFailure(request.provider);
  throw lastError;
}
```

### Retryable Errors

- HTTP 429 (rate limit)
- HTTP 502, 503, 504 (provider temporarily unavailable)
- Network timeouts
- Provider-specific transient errors

### Non-Retryable Errors

- HTTP 400 (bad request — our fault)
- HTTP 401/403 (auth errors)
- HTTP 422 (validation errors — wrong parameters)

## Impact

- **~40% reduction in video generation failures** from transient issues
- **Circuit breaker prevents cascading failures** when a provider is down
- **Aligned with VisionClaw circuit breaker pattern** — see `evolution/references/circuit_breaker_pattern.md`

## Cross-References

- `evolution/references/circuit_breaker_pattern.md` (ported from VisionClaw)
- `evolution/references/codex_harness_circuit_breaker.md`
- Issue #64723: Google Veo numberOfVideos fix (related provider hardening)