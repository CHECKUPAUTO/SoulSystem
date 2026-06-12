# Discord Message Deduplication Pattern

**Date:** 2026-04-13  
**Source:** Night Cycle Report (00:52)  
**Status:** Proposal  
**Priority:** P0 (addresses regression #65581)  
**Bug Reference:** Issue #65581 — Duplicate Discord messages on every response  

## Problem

Discord channel dispatch can send duplicate messages under certain conditions (network retries, event replay, race conditions in multi-gateway scenarios). Users see doubled responses.

## Pattern: TTL-Backed Message ID Dedup

```typescript
class MessageDeduplicator {
  private recentIds: Map<string, number> = new Map(); // id → timestamp
  private readonly TTL_MS = 5000; // 5 seconds

  isDuplicate(messageId: string): boolean {
    const now = Date.now();
    
    // Purge expired entries
    for (const [id, ts] of this.recentIds) {
      if (now - ts > this.TTL_MS) this.recentIds.delete(id);
    }

    if (this.recentIds.has(messageId)) {
      return true; // Duplicate within TTL
    }
    
    this.recentIds.set(messageId, now);
    return false;
  }
}
```

## Implementation Points

1. **Apply at channel dispatch layer** — before calling Discord API
2. **Use message content hash as fallback** — when platform doesn't provide stable IDs
3. **TTL of 5s is sufficient** — covers network retry windows without excessive memory
4. **Per-channel instance** — avoid cross-channel dedup

## Alternative: Content-Based Dedup

For platforms without stable message IDs:

```typescript
function messageHash(content: string, channelId: string): string {
  // Fast hash for dedup — not cryptographic
  return `${channelId}:${content.length}:${content.slice(0, 64)}`;
}
```

## Related

- Issue #65581: Duplicate Discord messages on every response
- `stream_parser_resilience.md` — similar resilience pattern for streaming