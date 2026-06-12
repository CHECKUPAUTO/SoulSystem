# TTLMap Generic Extraction Pattern
**Source:** OpenEvolve Night Cycle Analysis (night_cycle_20250411_0031.md)
**Created:** 2026-04-11 by OpenEvolve Auto-Apply

## Overview

The TTLMap pattern from models-config-state.ts is an elegant solution for cached data with automatic expiration. This document extracts it as a reusable generic utility.

## TypeScript Implementation

```typescript
// src/utils/ttl-map.ts

export class TTLMap<K, V> extends Map<K, V> {
  private timeouts = new Map<K, ReturnType<typeof setTimeout>>();

  constructor(
    private ttlMs: number,
    private onExpire?: (key: K, value: V) => void
  ) {
    super();
  }

  set(key: K, value: V): this {
    // Clear existing timer
    this.clearTimeout(key);
    
    // Store value
    super.set(key, value);

    // Set expiration timer
    const timer = setTimeout(() => {
      const value = this.get(key);
      this.delete(key);
      this.onExpire?.(key, value as V);
    }, this.ttlMs);

    // Allow Node.js to exit even with pending timers
    if (timer.unref) {
      timer.unref();
    }

    this.timeouts.set(key, timer);
    return this;
  }

  delete(key: K): boolean {
    this.clearTimeout(key);
    return super.delete(key);
  }

  clear(): void {
    for (const timer of this.timeouts.values()) {
      clearTimeout(timer);
    }
    this.timeouts.clear();
    super.clear();
  }

  private clearTimeout(key: K): void {
    const timer = this.timeouts.get(key);
    if (timer) {
      clearTimeout(timer);
      this.timeouts.delete(key);
    }
  }
}

// Usage in models-config-state.ts
const modelRegistryCache = new TTLMap<string, ModelRegistry>(
  60000, // 60 second TTL
  (key, value) => {
    console.log(`Cache expired for ${key}`);
  }
);
```

## Python Implementation

See: `skills/shared/ttl_map.py`

```python
from skills.shared.ttl_map import TTLMap

# Cache with 60 second TTL
cache = TTLMap[str, dict](ttl_ms=60000)

cache.set("model:registry", {"models": [...]})
registry = cache.get("model:registry")

# With expiration callback
def on_expire(key, value):
    print(f"Cache expired: {key}")

cache = TTLMap(ttl_ms=30000, on_expire=on_expire)
```

## Key Features

1. **Automatic Cleanup**: No manual cache management needed
2. **Callback Support**: React to expirations
3. **Memory Safe**: Timers are properly cleaned up
4. **Thread Safe**: Python implementation uses locks
5. **Node.js Friendly**: Uses `unref()` to prevent blocking exit

## Comparison with Other Cache Options

| Approach | Pros | Cons |
|----------|------|------|
| TTLMap | Auto-cleanup, callback, simple | No LRU eviction |
| LRU Cache | Size-based eviction | No TTL |
| node-cache | Many features | External dependency |
| lru-cache | Mature, popular | No callbacks |

## When to Use TTLMap

✅ Good for:
- Session data with known lifetime
- Temporary computed values
- Registry caches that refresh periodically
- Rate limit tracking

❌ Not for:
- Persistent storage
- Large datasets (use size-bounded LRU)
- Data requiring persistence across restarts

## Testing

```typescript
describe('TTLMap', () => {
  it('should expire after TTL', async () => {
    const map = new TTLMap<string, string>(100); // 100ms TTL
    map.set('key', 'value');
    
    expect(map.get('key')).toBe('value');
    
    await new Promise(r => setTimeout(r, 150));
    expect(map.get('key')).toBeUndefined();
  });

  it('should call onExpire callback', async () => {
    const onExpire = jest.fn();
    const map = new TTLMap<string, string>(100, onExpire);
    
    map.set('key', 'value');
    await new Promise(r => setTimeout(r, 150));
    
    expect(onExpire).toHaveBeenCalledWith('key', 'value');
  });

  it('should reset TTL on re-set', async () => {
    const map = new TTLMap<string, string>(100);
    map.set('key', 'value1');
    
    await new Promise(r => setTimeout(r, 50));
    map.set('key', 'value2'); // Reset TTL
    
    await new Promise(r => setTimeout(r, 60));
    expect(map.get('key')).toBe('value2'); // Should still exist
    
    await new Promise(r => setTimeout(r, 60));
    expect(map.get('key')).toBeUndefined(); // Now expired
  });
});
```

## Integration Example: Config Cache

```typescript
// src/config/config-cache.ts
import { TTLMap } from '../utils/ttl-map';

interface ConfigCacheEntry {
  config: Record<string, unknown>;
  loadTime: number;
}

class ConfigCache {
  private cache = new TTLMap<string, ConfigCacheEntry>(
    300000, // 5 minute TTL
    (key, entry) => {
      console.log(`Config cache expired: ${key}`);
    }
  );

  get(key: string): ConfigCacheEntry | undefined {
    return this.cache.get(key);
  }

  set(key: string, config: Record<string, unknown>): void {
    this.cache.set(key, {
      config,
      loadTime: Date.now()
    });
  }

  invalidate(key: string): boolean {
    return this.cache.delete(key);
  }

  clear(): void {
    this.cache.clear();
  }
}

export const configCache = new ConfigCache();
```

## References

- Night Cycle: night_cycle_20250411_0031.md
- Source: models-config-state.ts
- Python Implementation: skills/shared/ttl_map.py
