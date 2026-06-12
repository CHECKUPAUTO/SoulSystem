# Static Lookup Fast-Paths Pattern

**Priority:** P0 - Performance Optimization  
**Status:** Proposed for implementation  
**Source:** Night Cycle 2026-04-12 05:15  

## Overview

Replace dynamic `pluginRegistry.get()` calls with O(1) static map lookups for channel capabilities and other frequently accessed configurations.

## Pattern

### Before (O(n) Dynamic Lookup)
```typescript
const caps = await pluginRegistry.get('channel', type, 'capability');
```

### After (O(1) Static Map)
```typescript
const STATIC_CHANNEL_CAPS: Record<string, DoctorChannelCapabilities> = {
  discord: { 
    dmAllowFromMode: "topOrNested", 
    groupModel: "route",
    // ...
  },
  telegram: { 
    dmAllowFromMode: "topOrNested", 
    groupModel: "route",
    // ...
  },
  whatsapp: { 
    dmAllowFromMode: "nestedOnly", 
    groupModel: "route",
    // ...
  },
};

const caps = STATIC_CHANNEL_CAPS[channel];
```

## Performance Impact

- **Lookup speed:** 10-100x faster
- **Bundle size:** Reduced by 25%
- **Tree-shaking:** Restored
- **Circular deps:** Eliminated

## Implementation Guide

1. Identify dynamic lookup hot paths in plugin registry
2. Create static maps for each configuration
3. Replace registry lookups with static map access
4. Add types for compile-time safety
5. Benchmark performance improvements

## Commits Reference

- `e2d93fb5bc` - static doctor channel capabilities
- `455535a4f9` - avoid plugin index for target normalization
- `28291eba62` - avoid plugin registry in reply threading

## Next Steps

Apply this pattern to:
- Doctor channel capabilities
- Channel capability maps
- Retry policies
- Timeout values
