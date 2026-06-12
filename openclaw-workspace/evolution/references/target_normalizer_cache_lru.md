# Target Normalizer Cache LRU Guard

**Date:** 2026-04-13  
**Priority:** P1  
**Origin:** night_cycle_20260413_0015

## Problem

`target-normalization.ts` uses a version-keyed cache (`targetNormalizerCacheByChannelId`) that grows unbounded. If many channels are dynamically loaded, this map can consume unbounded memory.

```typescript
// Current: unbounded Map
const targetNormalizerCacheByChannelId = new Map<string, TargetNormalizerCacheEntry>();
```

## Recommendation

Add an LRU eviction or max-size guard to prevent unbounded growth:

```typescript
// Option 1: Simple max-size eviction
const MAX_CACHE_SIZE = 50;
function evictIfNeeded(cache: Map<string, TargetNormalizerCacheEntry>) {
  if (cache.size > MAX_CACHE_SIZE) {
    // Remove oldest entries (first inserted)
    const iterator = cache.keys();
    const toRemove = cache.size - MAX_CACHE_SIZE;
    for (let i = 0; i < toRemove; i++) {
      cache.delete(iterator.next().value);
    }
  }
}

// Option 2: LRU with access-time tracking
interface LRUCacheEntry<T> extends TargetNormalizerCacheEntry {
  lastAccessed: number;
}
const MAX_CACHE_SIZE = 50;
```

## Related

- `performance_optimization_patterns.md` — static lookup optimization
- `barrel_bypassing_guide.md` — direct import patterns that reduce cache pressure
- Version-based invalidation via `getActivePluginChannelRegistryVersion()` — needs test coverage