# Circuit Breaker Pattern

**Source:** OpenEvolve Night Cycle Report 2026-04-12 01:15 UTC  
**Priority:** P0 - Critical Reliability  
**Use Case:** Prevent Context Engine latency cascade failures

---

## Problem Statement

Current OpenClaw routing layer has no circuit breakers. If Context Engine experiences latency:

1. Requests queue up waiting for Context
2. Gateway threads exhaust
3. Entire loop stalls
4. Cascading failure across all components

**T430 Fitness Score:** Syntax: N/A | Semantic: 0.60 | Quality: 0.70 | Security: 0.85 | **Total: 0.72**

---

## Solution: Circuit Breaker Pattern

```
┌──────────────┐     ┌──────────────┐     ┌──────────────┐
│   Request    │────▶│   Circuit    │────▶│   Service    │
│              │     │   Breaker    │     │ (Context)    │
└──────────────┘     └──────────────┘     └──────────────┘
                            │
                            ▼
                     ┌──────────────┐
                     │   Fallback   │
                     │   Response   │
                     └──────────────┘
```

---

## Implementation

### Core Circuit Breaker

```typescript
// src/resilience/circuit-breaker.ts

type CircuitState = 'CLOSED' | 'OPEN' | 'HALF_OPEN';

interface CircuitBreakerConfig {
  failureThreshold: number;      // Failures before opening
  resetTimeout: number;          // Time before half-open (ms)
  halfOpenMaxCalls: number;      // Test calls in half-open
  successThreshold: number;       // Successes to close
}

interface CircuitMetrics {
  failures: number;
  successes: number;
  lastFailureTime: Date | null;
  state: CircuitState;
  halfOpenCalls: number;
}

class CircuitBreaker {
  private state: CircuitState = 'CLOSED';
  private failures = 0;
  private successes = 0;
  private lastFailureTime: Date | null = null;
  private halfOpenCalls = 0;
  
  constructor(
    private config: CircuitBreakerConfig,
    private name: string
  ) {}
  
  async execute<T>(
    fn: () => Promise<T>,
    fallback: T
  ): Promise<T> {
    
    if (this.state === 'OPEN') {
      if (this.shouldAttemptReset()) {
        this.transitionTo('HALF_OPEN');
      } else {
        return fallback;
      }
    }
    
    if (this.state === 'HALF_OPEN') {
      if (this.halfOpenCalls >= this.config.halfOpenMaxCalls) {
        return fallback;
      }
      this.halfOpenCalls++;
    }
    
    try {
      const result = await fn();
      this.onSuccess();
      return result;
    } catch (error) {
      this.onFailure();
      return fallback;
    }
  }
  
  private onSuccess(): void {
    this.failures = 0;
    
    if (this.state === 'HALF_OPEN') {
      this.successes++;
      if (this.successes >= this.config.successThreshold) {
        this.transitionTo('CLOSED');
      }
    }
  }
  
  private onFailure(): void {
    this.failures++;
    this.lastFailureTime = new Date();
    this.successes = 0;
    
    if (this.state === 'HALF_OPEN') {
      this.transitionTo('OPEN');
    } else if (this.failures >= this.config.failureThreshold) {
      this.transitionTo('OPEN');
    }
  }
  
  private shouldAttemptReset(): boolean {
    if (!this.lastFailureTime) return true;
    const elapsed = Date.now() - this.lastFailureTime.getTime();
    return elapsed >= this.config.resetTimeout;
  }
  
  private transitionTo(newState: CircuitState): void {
    const oldState = this.state;
    this.state = newState;
    
    if (newState === 'HALF_OPEN') {
      this.halfOpenCalls = 0;
      this.successes = 0;
    }
    
    if (newState === 'CLOSED') {
      this.failures = 0;
      this.halfOpenCalls = 0;
    }
    
    this.emitEvent('state-change', { oldState, newState });
  }
  
  getMetrics(): CircuitMetrics {
    return {
      failures: this.failures,
      successes: this.successes,
      lastFailureTime: this.lastFailureTime,
      state: this.state,
      halfOpenCalls: this.halfOpenCalls
    };
  }
  
  private emitEvent(event: string, data: unknown): void {
    // Telemetry hook
  }
}
```

### Context Engine Integration

```typescript
// src/context/circuit-breaker-wrapper.ts
import { CircuitBreaker } from '../resilience/circuit-breaker';

const contextBreaker = new CircuitBreaker({
  name: 'context-engine',
  failureThreshold: 5,
  resetTimeout: 30000,      // 30 seconds
  halfOpenMaxCalls: 3,
  successThreshold: 2
});

export async function fetchContextWithCircuitBreaker(
  request: ContextRequest
): Promise<ContextResponse> {
  return contextBreaker.execute(
    () => fetchContext(request),
    { 
      // Fallback: degraded context
      user: request.user,
      preferences: {},
      history: [],
      _degraded: true,
      _reason: 'Circuit breaker OPEN'
    }
  );
}
```

### Gateway Integration

```typescript
// src/gateway/circuit-breaker-middleware.ts
import { CircuitBreaker } from '../resilience/circuit-breaker';

const breakers = new Map<string, CircuitBreaker>();

export function createCircuitBreakerMiddleware(
  serviceName: string,
  config: CircuitBreakerConfig
) {
  if (!breakers.has(serviceName)) {
    breakers.set(serviceName, new CircuitBreaker(config, serviceName));
  }
  
  const breaker = breakers.get(serviceName)!;
  
  return async function circuitBreakerMiddleware(
    req: Request,
    next: () => Promise<Response>
  ): Promise<Response> {
    return breaker.execute(
      () => next(),
      {
        status: 503,
        body: JSON.stringify({
          error: 'Service temporarily unavailable',
          service: serviceName,
          circuitState: breaker.getMetrics().state
        })
      }
    );
  };
}
```

---

## State Machine

```
                    ┌─────────────────────────────────────────┐
                    │                                         │
                    ▼                                         │
┌──────────┐    ┌──────────┐    ┌──────────┐    ┌──────────┐│
│  CLOSED  │───▶│   OPEN   │───▶│ HALF_OPEN│───▶│  CLOSED  │┘
│  (normal)│    │ (fail)   │    │ (testing)│    │ (recovered)
└──────────┘    └──────────┘    └──────────┘    └──────────┘
     │                              │
     │                              │
     └──────────────────────────────┘
          (failure in half-open)
```

| State | Behavior |
|-------|----------|
| **CLOSED** | Normal operation. Count failures, open if threshold exceeded. |
| **OPEN** | Fast-fail with fallback. Wait for timeout, then try half-open. |
| **HALF_OPEN** | Limited test calls. Close on success threshold, open on failure. |

---

## Configuration Presets

```typescript
// src/resilience/circuit-breaker-presets.ts

export const CircuitBreakerPresets = {
  // For critical path components
  critical: {
    failureThreshold: 3,
    resetTimeout: 15000,
    halfOpenMaxCalls: 2,
    successThreshold: 2
  },
  
  // For non-critical components
  lenient: {
    failureThreshold: 10,
    resetTimeout: 60000,
    halfOpenMaxCalls: 5,
    successThreshold: 3
  },
  
  // For external APIs
  external: {
    failureThreshold: 5,
    resetTimeout: 30000,
    halfOpenMaxCalls: 3,
    successThreshold: 2
  }
} as const;
```

---

## Monitoring

```typescript
// src/resilience/circuit-breaker-metrics.ts

interface CircuitBreakerEvent {
  service: string;
  event: 'state-change' | 'failure' | 'success' | 'fallback';
  timestamp: Date;
  details: unknown;
}

class CircuitBreakerMonitor {
  private events: CircuitBreakerEvent[] = [];
  
  record(event: CircuitBreakerEvent): void {
    this.events.push(event);
    
    // Alert on OPEN
    if (event.event === 'state-change' && 
        (event.details as { newState: string }).newState === 'OPEN') {
      this.alert(`Circuit breaker OPEN for ${event.service}`);
    }
  }
  
  getDashboard(): CircuitMetrics[] {
    return Array.from(breakers.entries()).map(([name, breaker]) => ({
      service: name,
      ...breaker.getMetrics()
    }));
  }
}
```

---

## Testing

```typescript
// src/resilience/circuit-breaker.test.ts

describe('CircuitBreaker', () => {
  it('should close circuit on success', async () => {
    const breaker = new CircuitBreaker({
      failureThreshold: 3,
      resetTimeout: 1000,
      halfOpenMaxCalls: 2,
      successThreshold: 1
    }, 'test');
    
    const result = await breaker.execute(
      () => Promise.resolve('success'),
      'fallback'
    );
    
    expect(result).toBe('success');
    expect(breaker.getMetrics().state).toBe('CLOSED');
  });
  
  it('should open circuit after failures', async () => {
    const breaker = new CircuitBreaker({
      failureThreshold: 2,
      resetTimeout: 10000,
      halfOpenMaxCalls: 2,
      successThreshold: 1
    }, 'test');
    
    // Fail twice
    await breaker.execute(() => Promise.reject(new Error('fail')), 'fallback');
    await breaker.execute(() => Promise.reject(new Error('fail')), 'fallback');
    
    expect(breaker.getMetrics().state).toBe('OPEN');
    
    // Should return fallback immediately
    const result = await breaker.execute(
      () => Promise.resolve('should not reach'),
      'fallback'
    );
    expect(result).toBe('fallback');
  });
  
  it('should transition to half-open after timeout', async () => {
    const breaker = new CircuitBreaker({
      failureThreshold: 1,
      resetTimeout: 50,  // Very short for test
      halfOpenMaxCalls: 2,
      successThreshold: 1
    }, 'test');
    
    // Open the circuit
    await breaker.execute(() => Promise.reject(new Error('fail')), 'fallback');
    expect(breaker.getMetrics().state).toBe('OPEN');
    
    // Wait for timeout
    await new Promise(r => setTimeout(r, 100));
    
    // Next call should attempt in half-open
    const result = await breaker.execute(
      () => Promise.resolve('success'),
      'fallback'
    );
    
    expect(result).toBe('success');
    expect(breaker.getMetrics().state).toBe('CLOSED');
  });
});
```

---

## References

- Night Cycle Report: night_cycle_20260412_0115.md
- IronReview T430 Analysis
- Circuit Breaker Pattern (Martin Fowler)
- Release It! (Michael Nygard)