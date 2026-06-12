# Command Registry Cache Cohesion Pattern

**Priority:** Low  
**Source:** Night Cycle 2026-04-13 06:30  
**Status:** Documented — awaiting implementation decision  

## Problem

`commands-registry-normalize.ts` uses multiple module-level `let` bindings for caches (`cachedTextAliasMap`, `cachedTextAliasCommands`, `cachedDetection`, `cachedDetectionCommands`) with referential equality invalidation. While currently correct (since `getChatCommands()` returns a new array on config change), this pattern is fragile — any future refactoring that mutates the commands array in-place would silently break cache invalidation.

## Current Implementation

```typescript
// Fragile: module-level let bindings with referential invalidation
let cachedTextAliasMap: Map<string, ChatCommandDefinition> | undefined;
let cachedTextAliasCommands: ChatCommandDefinition[] | undefined;
let cachedDetection: Map<string, ChatCommandDefinition> | undefined;
let cachedDetectionCommands: ChatCommandDefinition[] | undefined;

function invalidate() {
  cachedTextAliasMap = undefined;
  cachedTextAliasCommands = undefined;
  cachedDetection = undefined;
  cachedDetectionCommands = undefined;
}
```

## Proposed Solutions

### Option A: CommandRegistryCache Class

```typescript
class CommandRegistryCache {
  private textAliasMap?: Map<string, ChatCommandDefinition>;
  private textAliasCommands?: ChatCommandDefinition[];
  private detection?: Map<string, ChatCommandDefinition>;
  private detectionCommands?: ChatCommandDefinition[];
  
  invalidate(): void {
    this.textAliasMap = undefined;
    this.textAliasCommands = undefined;
    this.detection = undefined;
    this.detectionCommands = undefined;
  }
  
  // Typed accessors with build-if-miss semantics
  getTextAliasMap(commands: ChatCommandDefinition[]): Map<string, ChatCommandDefinition> {
    if (!this.textAliasMap) this.textAliasMap = buildTextAliasMap(commands);
    return this.textAliasMap;
  }
  // ... etc
}
```

**Benefit:** Single `invalidate()` method, encapsulated state, type-safe accessors.

### Option B: WeakMap<ChatCommandDefinition[], CacheEntry>

```typescript
const cache = new WeakMap<ChatCommandDefinition[], CacheEntry>();

function getCache(commands: ChatCommandDefinition[]): CacheEntry {
  let entry = cache.get(commands);
  if (!entry) {
    entry = buildCacheEntry(commands);
    cache.set(commands, entry);
  }
  return entry;
}
```

**Benefit:** Automatic garbage collection when commands array is replaced. No manual invalidation needed.

**Risk:** WeakMap doesn't support enumeration, making debugging harder.

## Recommendation

Option A (CommandRegistryCache class) is preferred for:
- Explicit invalidation control
- Easier debugging and logging
- Clear single-responsibility boundary
- Future extensibility (TTL, size limits, metrics)

## Cross-References

- `barrel_bypassing_guide.md` — The commands-registry extraction follows barrel bypass principles
- `narrow_surface_pattern.md` — The normalization module is a good example of narrow surface design
- `startup_context_extraction_pattern.md` — Related to how commands are loaded during startup

## Related Commits

- `eb9d8d41cc` — Commands registry normalization extracted from monolith
- Vincent Koc's barrel bypass campaign (~60% complete as of 2026-04-13)