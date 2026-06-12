# Plugin Import Best Practices

**Generated:** 2026-04-11 10:41 UTC  
**Source:** OpenEvolve Night Cycle Analysis (kimi-k2.5)  
**Status:** Documentation - Safe to Apply

## Overview

Based on analysis of recent OpenClaw commits, a clear pattern emerged: **avoid plugin barrel exports in favor of direct source imports**. This document codifies that pattern for future development.

## The Pattern

### ❌ Avoid: Barrel Import (index.js)

```typescript
// DON'T: Import through plugin barrel
import { normalizeChannelId } from "../../channels/plugins/index.js";
import { getCapability } from "../../capabilities/index.js";
```

### ✅ Prefer: Direct Source Import

```typescript
// DO: Import directly from source
import { normalizeAnyChannelId } from "../../channels/registry.js";
import { getStaticCapability } from "../../capabilities/static.js";
```

## Why This Matters

1. **Runtime Overhead**: Barrel exports add indirection layers
2. **Bundle Size**: Dead code elimination works better with direct imports
3. **Cold Start**: CLI and gateway startup latency reduced
4. **Clarity**: Source of truth is explicit

## Static Lookup Short-Circuiting

For hot paths, consider pre-baked lookup tables:

```typescript
// O(1) lookup for known channels
const STATIC_DOCTOR_CHANNEL_CAPABILITIES = {
  matrix: { dmAllowFromMode: "nestedOnly", groupModel: "sender", ... },
  msteams: { dmAllowFromMode: "topOnly", groupModel: "hybrid", ... },
  zalouser: { dmAllowFromMode: "topOnly", groupModel: "hybrid", ... },
};

function getChannelCapability(channelId: string) {
  // Fast path: static lookup
  if (STATIC_DOCTOR_CHANNEL_CAPABILITIES[channelId]) {
    return STATIC_DOCTOR_CHANNEL_CAPABILITIES[channelId];
  }
  // Fallback: dynamic plugin resolution
  return resolveFromPluginRegistry(channelId);
}
```

## Guidelines

### When to Use Direct Imports

- Hot paths (message routing, capability checks)
- Known channel types with stable configurations
- Performance-critical code sections

### When Barrel Exports Are OK

- Plugin development (external authors)
- Configuration-time code (not runtime)
- Backward compatibility layers

## Testing Considerations

When mocking in tests, prefer targeted over global:

```typescript
// GOOD: Targeted mock
jest.mock("../../channels/registry.js", () => ({
  normalizeAnyChannelId: jest.fn((id) => `normalized-${id}`),
}));

// AVOID: Global stub that affects all imports
jest.mock("../../channels/plugins/index.js", ...);
```

## Migration Checklist

- [ ] Identify hot path imports in your code
- [ ] Replace barrel imports with direct source imports
- [ ] Add static lookup tables for frequently accessed items
- [ ] Update tests to mock at the correct level
- [ ] Benchmark cold-start performance before/after

## References

- OpenClaw commits: `e2d93fb5bc`, `455535a4f9`, `28291eba62`
- Pattern: Zero-cost abstraction via static lookup
- Risk: Tight coupling to internal structure (document dependencies)
