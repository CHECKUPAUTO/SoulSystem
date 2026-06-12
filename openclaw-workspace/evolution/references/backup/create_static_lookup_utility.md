# createStaticLookup<T>() Utility Pattern

**Source:** OpenEvolve Night Cycle Reports 2026-04-11 (2346, 2115)  
**Purpose:** Type-safe generic utility to eliminate repetitive static map + fallback patterns

## Problem Statement

Multiple locations in OpenClaw use a repetitive pattern:

```typescript
// Before: Repetitive boilerplate
const STATIC_LOOKUP: Record<string, SomeType> = {
  'key1': value1,
  'key2': value2,
};

function getSomething(key: string): SomeType {
  return STATIC_LOOKUP[key] ?? defaultValue;
}
```

This pattern is repeated across:
- `src/commands/doctor/channel-capabilities.ts`
- `src/agents/system-prompt.ts`
- `src/channels/registry.ts`

## Solution

```typescript
// src/utils/static-lookup.ts

export interface StaticLookupOptions<T> {
  fallback: T;
  keyTransform?: (key: string) => string;
}

export class StaticLookup<T> {
  private map: Readonly<Map<string, T>>;
  private fallback: T;
  private keyTransform: (key: string) => string;
  
  constructor(
    entries: Iterable<[string, T]>,
    options: StaticLookupOptions<T>
  ) {
    this.map = new Map(entries);
    this.fallback = options.fallback;
    this.keyTransform = options.keyTransform ?? ((k) => k.toLowerCase());
  }
  
  get(key: string): T {
    const normalized = this.keyTransform(key);
    return this.map.get(normalized) ?? this.fallback;
  }
  
  has(key: string): boolean {
    return this.map.has(this.keyTransform(key));
  }
  
  keys(): IterableIterator<string> {
    return this.map.keys();
  }
  
  entries(): IterableIterator<[string, T]> {
    return this.map.entries();
  }
}

// Factory function
export function createStaticLookup<T>(
  entries: Record<string, T> | Array<[string, T]>,
  options: StaticLookupOptions<T>
): StaticLookup<T> {
  const iterable = Array.isArray(entries) 
    ? entries 
    : Object.entries(entries);
  return new StaticLookup(iterable, options);
}
```

## Usage Examples

### Channel Capabilities

```typescript
// Before
const STATIC_DOCTOR_CHANNEL_CAPABILITIES: Record<ChannelType, ChannelCapabilities> = {
  telegram: { supportsReactions: true, supportsThreads: true },
  discord: { supportsReactions: true, supportsThreads: true },
  // ...
};

// After
const channelCapabilities = createStaticLookup(
  {
    telegram: { supportsReactions: true, supportsThreads: true },
    discord: { supportsReactions: true, supportsThreads: true },
    // ...
  },
  {
    fallback: { supportsReactions: false, supportsThreads: false },
    keyTransform: (k) => k.toLowerCase()
  }
);

// Usage
const caps = channelCapabilities.get(channelType);
```

### System Prompt Channel Logic

```typescript
// Before
const STATIC_NON_NATIVE_APPROVAL_CHANNELS = ['telegram', 'discord', 'whatsapp'];

function needsApproval(channel: string): boolean {
  return STATIC_NON_NATIVE_APPROVAL_CHANNELS.includes(channel.toLowerCase());
}

// After
const approvalConfig = createStaticLookup(
  [
    ['telegram', { needsApproval: true, autoApprove: false }],
    ['discord', { needsApproval: true, autoApprove: false }],
    ['local', { needsApproval: false, autoApprove: true }],
  ],
  {
    fallback: { needsApproval: true, autoApprove: false },
    keyTransform: (k) => k.toLowerCase()
  }
);

// Usage
const config = approvalConfig.get(channelType);
if (config.needsApproval) { /* ... */ }
```

### Registry Short-Circuiting

```typescript
// Combined with dynamic fallback
const staticRegistry = createStaticLookup(
  [
    ['telegram', telegramPlugin],
    ['discord', discordPlugin],
  ],
  {
    fallback: null,
    keyTransform: (k) => k.toLowerCase()
  }
);

export function getPlugin(channelType: string): ChannelPlugin {
  // O(1) static lookup first
  const static = staticRegistry.get(channelType);
  if (static) return static;
  
  // O(n) dynamic fallback
  return dynamicRegistry.find(p => p.type === channelType);
}
```

## Benefits

1. **Type Safety:** Generic T ensures type consistency
2. **Normalization:** Automatic key transformation (case-insensitive by default)
3. **Immutability:** Readonly map prevents accidental mutations
4. **Testability:** Easy to mock and inject
5. **Performance:** O(1) lookups vs O(n) traversal

## Migration Path

1. Create `src/utils/static-lookup.ts`
2. Identify all static lookup patterns
3. Replace one module at a time
4. Run tests after each replacement
5. Remove old static constants

## Testing

```typescript
// static-lookup.test.ts
describe('createStaticLookup', () => {
  it('returns value for exact key match', () => {
    const lookup = createStaticLookup({ a: 1 }, { fallback: 0 });
    expect(lookup.get('a')).toBe(1);
  });
  
  it('returns fallback for missing key', () => {
    const lookup = createStaticLookup({ a: 1 }, { fallback: 0 });
    expect(lookup.get('z')).toBe(0);
  });
  
  it('applies key transformation', () => {
    const lookup = createStaticLookup(
      { ABC: 1 },
      { fallback: 0, keyTransform: (k) => k.toUpperCase() }
    );
    expect(lookup.get('abc')).toBe(1);
  });
});
```

## References

- Night Cycle Reports: night_cycle_20260411_2346.md, night_cycle_20260411_2115.md
- Pattern: "Barrel Avoidance" and "Static Lookup Short-Circuit"
