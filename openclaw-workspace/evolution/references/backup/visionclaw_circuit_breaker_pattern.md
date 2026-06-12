# VisionClaw Circuit Breaker Pattern

**Status:** Reference Documentation  
**Source:** night_cycle_20260412_0215.md, night_cycle_20260412_0245.md  
**Priority:** P0 - Critical Reliability Pattern  
**Auto-Apply:** ❌ NO - Requires Core Gateway Modifications  

## Overview

VisionClaw implemented a circuit breaker pattern to prevent infinite tool call retry loops. This pattern should be ported to OpenClaw core for improved reliability.

## Problem

Infinite retry loops in tool calls can:
- Exhaust system resources
- Cause cascading failures across the agent ecosystem
- Degrade user experience with unresponsive agents

## Solution: Circuit Breaker Pattern

### State Machine

```typescript
enum CircuitState {
  CLOSED,      // Normal operation - requests pass through
  OPEN,        // Failing fast - reject calls immediately
  HALF_OPEN    // Testing recovery - limited test calls allowed
}

interface CircuitBreakerConfig {
  failureThreshold: number;    // Max failures before opening (default: 5)
  recoveryTimeout: number;     // Time before half-open (default: 60000ms)
  halfOpenMaxCalls: number;    // Test calls in half-open state (default: 3)
}
```

### Implementation

```typescript
class CircuitBreaker {
  private state: CircuitState = CircuitState.CLOSED;
  private failures = 0;
  private lastFailureTime?: number;
  private halfOpenCalls = 0;

  constructor(private config: CircuitBreakerConfig) {}

  async call<T>(fn: () => Promise<T>): Promise<T> {
    if (this.state === CircuitState.OPEN) {
      if (Date.now() - (this.lastFailureTime || 0) > this.config.recoveryTimeout) {
        this.state = CircuitState.HALF_OPEN;
        this.halfOpenCalls = 0;
      } else {
        throw new CircuitOpenError('Circuit breaker is OPEN');
      }
    }

    if (this.state === CircuitState.HALF_OPEN) {
      if (this.halfOpenCalls >= this.config.halfOpenMaxCalls) {
        throw new CircuitOpenError('Circuit breaker HALF_OPEN limit reached');
      }
      this.halfOpenCalls++;
    }

    try {
      const result = await fn();
      this.onSuccess();
      return result;
    } catch (error) {
      this.onFailure();
      throw error;
    }
  }

  private onSuccess(): void {
    this.failures = 0;
    if (this.state === CircuitState.HALF_OPEN) {
      this.state = CircuitState.CLOSED;
      this.halfOpenCalls = 0;
    }
  }

  private onFailure(): void {
    this.failures++;
    this.lastFailureTime = Date.now();
    
    if (this.failures >= this.config.failureThreshold) {
      this.state = CircuitState.OPEN;
    }
  }

  getState(): CircuitState {
    return this.state;
  }
}

class CircuitOpenError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'CircuitOpenError';
  }
}
```

## Integration Points

### 1. Tool Execution Layer

```typescript
// src/execution/tool-executor.ts
const circuitBreaker = new CircuitBreaker({
  failureThreshold: 5,
  recoveryTimeout: 60000,
  halfOpenMaxCalls: 3
});

export async function executeTool(toolCall: ToolCall): Promise<ToolResult> {
  return circuitBreaker.call(async () => {
    // Actual tool execution logic
    return await invokeTool(toolCall);
  });
}
```

### 2. Gateway Routing Layer

```typescript
// src/gateway/routing/circuit-breaker-middleware.ts
export function circuitBreakerMiddleware(
  config: CircuitBreakerConfig
): Middleware {
  const breakers = new Map<string, CircuitBreaker>();

  return async (ctx, next) => {
    const route = ctx.request.path;
    
    if (!breakers.has(route)) {
      breakers.set(route, new CircuitBreaker(config));
    }
    
    const breaker = breakers.get(route)!;
    return breaker.call(() => next());
  };
}
```

### 3. Channel-Specific Breakers

Different channels may need different thresholds:

```typescript
const channelConfigs: Record<ChannelType, CircuitBreakerConfig> = {
  telegram: { failureThreshold: 10, recoveryTimeout: 30000, halfOpenMaxCalls: 5 },
  discord: { failureThreshold: 8, recoveryTimeout: 45000, halfOpenMaxCalls: 3 },
  whatsapp: { failureThreshold: 5, recoveryTimeout: 60000, halfOpenMaxCalls: 2 },
  // ... other channels
};
```

## Metrics & Observability

```typescript
interface CircuitBreakerMetrics {
  state: CircuitState;
  failureCount: number;
  lastFailureTime?: number;
  successCount: number;
  rejectionCount: number;
}

// Export to Prometheus/OpenTelemetry
function exportMetrics(breaker: CircuitBreaker): CircuitBreakerMetrics {
  return {
    state: breaker.getState(),
    // ... other metrics
  };
}
```

## Benefits

1. **Fail Fast**: Prevents resource exhaustion from repeated failed calls
2. **Self-Healing**: Automatically tests recovery after timeout
3. **Observability**: Clear state visibility for debugging
4. **Graceful Degradation**: Can return cached values or defaults when open

## Why Manual Implementation Required

This pattern requires:
- New `src/resilience/` module creation
- Integration with existing retry logic
- Metrics/observability hooks
- Configuration schema updates
- Comprehensive testing

## References

- VisionClaw Implementation: `VisionClaw/GeminiLiveService.swift`
- Original Commit: `3268d72` (VisionClaw)
- Pattern Reference: [Microsoft Circuit Breaker](https://docs.microsoft.com/en-us/azure/architecture/patterns/circuit-breaker)
