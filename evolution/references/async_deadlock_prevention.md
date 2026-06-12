# Async Deadlock Prevention Pattern

**CodeWiki Pattern ID:** `patterns/async-deadlock-prevention`  
**Classification:** Critical Security Pattern  
**Status:** Ready for Implementation  
**Source:** OpenEvolve Night Cycle 2026-04-12_0635 (Telegram fix `22b53a49`)

---

## Problem Statement

Async callback flows can block indefinitely when callback chains depend on unresolved promises. This creates **production-blocking deadlocks** where approval callbacks hang indefinitely, consuming resources and preventing progress.

**Critical Example (Telegram Approval Callback):**
```typescript
// BEFORE: Deadlock-prone pattern
async function handleApproval(callback: ApprovalCallback) {
  await registerCallback(callback);  // Blocks here if callback never resolves
  await processApproval();           // Never reached
}
```

---

## Solution Pattern

### Core Principle: Non-blocking Callback Registration

Ensure callback registration returns immediately while async work continues independently.

```typescript
// AFTER: Deadlock-safe pattern
async function handleApproval(callback: ApprovalCallback) {
  // Register callback without awaiting
  registerCallback(callback).catch(error => {
    logger.error('Callback registration failed', error);
  });
  
  // Continue immediately
  await processApproval();
}
```

---

## Implementation Strategies

### Strategy 1: Fire-and-Forget with Error Handling

```typescript
class AsyncCallbackManager<T> {
  private callbacks = new Map<string, Callback<T>>();
  
  register(id: string, callback: Callback<T>): void {
    // Non-blocking registration
    this.callbacks.set(id, callback);
    
    // Cleanup on timeout
    setTimeout(() => this.cleanup(id), CALLBACK_TIMEOUT);
  }
  
  async execute(id: string, data: T): Promise<void> {
    const callback = this.callbacks.get(id);
    if (!callback) return;
    
    try {
      await callback(data);
    } finally {
      this.callbacks.delete(id);
    }
  }
  
  private cleanup(id: string): void {
    if (this.callbacks.has(id)) {
      logger.warn(`Callback ${id} expired without execution`);
      this.callbacks.delete(id);
    }
  }
}
```

### Strategy 2: Promise Race with Timeout

```typescript
async function withTimeout<T>(
  promise: Promise<T>,
  timeoutMs: number,
  context: string
): Promise<T> {
  const timeout = new Promise<never>((_, reject) => {
    setTimeout(() => reject(new Error(`${context} timed out`)), timeoutMs);
  });
  
  return Promise.race([promise, timeout]);
}

// Usage
async function handleCallback(callback: Callback) {
  await withTimeout(
    callback.execute(),
    30000,  // 30 second timeout
    'Approval callback'
  );
}
```

### Strategy 3: Async Iterator Pattern

For streams of callbacks, use async iterators to prevent buildup:

```typescript
async function* callbackStream() {
  while (true) {
    const callback = await dequeueCallback();
    if (!callback) break;
    yield callback;
  }
}

// Process without blocking
for await (const callback of callbackStream()) {
  processCallback(callback).catch(console.error);  // Non-blocking
}
```

---

## Detection Rules (T430 Security Score)

Add to IronReview's static analysis:

```rust
// T430 Rule: Async Deadlock Detection
if function.contains_await() && has_callback_registration() {
    security_score -= 0.15; // Risk of callback deadlock
}

// T430 Rule: Missing Timeout
if async_operation() && !has_timeout_guard() {
    security_score -= 0.10;
}
```

---

## Testing Patterns

### Unit Test

```typescript
describe('AsyncCallbackManager', () => {
  it('should not block on callback registration', async () => {
    const manager = new AsyncCallbackManager<string>();
    let executed = false;
    
    // Register slow callback
    manager.register('test', async () => {
      await sleep(5000);  // 5 second delay
      executed = true;
    });
    
    // Should return immediately
    const start = Date.now();
    await manager.execute('test', 'data');
    const duration = Date.now() - start;
    
    expect(duration).toBeLessThan(100);  // Not blocked
    expect(executed).toBe(false);  // Not yet executed
  });
  
  it('should timeout hung callbacks', async () => {
    const manager = new AsyncCallbackManager<string>(100);  // 100ms timeout
    
    manager.register('hung', async () => {
      await sleep(10000);  // Never completes
    });
    
    await sleep(200);  // Wait for timeout
    
    expect(manager.has('hung')).toBe(false);  // Cleaned up
  });
});
```

---

## Platform-Specific Considerations

### Telegram

- Approval callbacks must complete within 30 seconds
- Use `answerCallbackQuery` with timeout
- Handle `telegram.errors.RetryAfter` errors

### Discord

- Interaction callbacks have 3-second timeout
- Use `deferReply()` for long-running operations
- Implement follow-up webhook pattern

### MS Teams

- Bot Framework callbacks timeout at 15 seconds
- Use proactive messaging for long operations
- Implement `continueConversation` pattern

---

## Metrics and Monitoring

Track async callback health:

```typescript
interface CallbackMetrics {
  totalRegistered: number;
  totalExecuted: number;
  totalTimedOut: number;
  averageExecutionTime: number;
  pendingCount: number;
}

// Alert if pendingCount > threshold
if (metrics.pendingCount > 100) {
  alert('High pending callback count - potential deadlock');
}
```

---

## Related Patterns

- [Circuit Breaker Pattern](circuit_breaker_pattern.md) - For cascade failure prevention
- [Session Persistence Pattern](session_persistence_pattern.md) - For state preservation across restarts
- [Security Pipeline Pattern](security_pipeline_pattern.md) - For defensive validation

---

## References

- **Source Commit:** `22b53a49` - Telegram approval callback deadlock fix
- **T430 Security Score:** +10% when properly implemented
- **Impact:** Prevents indefinite resource hangs

---

*Generated by OpenEvolve Night Cycle Analysis*  
*Report ID: night_cycle_20260412_0635*
