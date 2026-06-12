# Stream Parser Resilience Pattern

**Date:** 2026-04-13  
**Source:** Night Cycle Report (00:52)  
**Status:** Proposal  
**Priority:** P0 (addresses crash bug #65566)  
**Bug Reference:** Issue #65566 — Streaming partialParse JSON errors crash agent runs  

## Problem

When LLM providers send incomplete or malformed JSON chunks during streaming, `JSON.parse` calls on partial chunks throw unhandled errors that crash entire agent runs. This is a P0 reliability issue.

## Pattern: Buffered Partial Parse Recovery

```typescript
class StreamingJsonParser {
  private buffer = '';
  private recentIds: Set<string> = new Set(); // TTL cache for dedup

  feed(chunk: string): ParseResult | null {
    this.buffer += chunk;
    
    try {
      const result = JSON.parse(this.buffer);
      this.buffer = ''; // Clear on success
      return { success: true, data: result };
    } catch (e) {
      if (this.isPartialJsonError(e)) {
        // Buffer and wait for more data
        return null; // Not an error, just incomplete
      }
      // Genuine parse error — log and discard
      this.buffer = '';
      return { success: false, error: e.message };
    }
  }

  private isPartialJsonError(e: SyntaxError): boolean {
    // Common indicators of partial JSON rather than corrupt JSON
    return e.message.includes('Unexpected end') || 
           e.message.includes('Expected');
  }
}
```

## Alternative: try/catch Wrapper

For simpler cases, wrap all streaming JSON parse attempts:

```typescript
function safeParseJson(text: string, context: string): unknown | null {
  try {
    return JSON.parse(text);
  } catch (e) {
    logger.warn(`JSON parse failure in ${context}`, { 
      error: e.message, 
      textLength: text.length,
      preview: text.slice(0, 100) 
    });
    return null; // Don't crash — return null and let caller decide
  }
}
```

## Guidelines

1. **Never let JSON.parse crash a run** — always wrap in try/catch
2. **Buffer partial chunks** — streaming protocols send fragments
3. **Log parse failures** with context for debugging
4. **Graceful degradation** — null result > crash

## Related Patterns

- `circuit_breaker_pattern.md` — Circuit breaker for cascading failures
- Discord dedup pattern (message ID Set with TTL) — similar resilience approach

## Upstream Tracking

- Issue #65566: Streaming partialParse JSON errors crash agent runs