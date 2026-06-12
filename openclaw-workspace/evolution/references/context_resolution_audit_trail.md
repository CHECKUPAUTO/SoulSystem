# Context Window Resolution Audit Trail
**Source:** OpenEvolve Night Cycle Analysis (night_cycle_20250411_0031.md)
**Created:** 2026-04-11 by OpenEvolve Auto-Apply

## Problem

Context resolution logic in context.ts has multiple fallback paths, making debugging difficult. Need visibility into why a particular context window size was chosen.

## Solution: Audit Trail

```typescript
// src/agents/context-resolution-audit.ts

type ResolutionSource = 
  | 'override'
  | 'configured-provider-window'
  | 'cache-qualified-key'
  | 'cache-bare-key'
  | 'fallback';

interface ContextResolution {
  tokens: number;
  source: ResolutionSource;
  provider?: string;
  model: string;
  timestamp: number;
  details?: Record<string, unknown>;
}

interface ResolutionAuditLog {
  resolutions: ContextResolution[];
  maxSize: number;
}
```

## Implementation

```typescript
class ContextResolutionAuditor {
  private log: ContextResolution[] = [];
  private readonly maxSize: number;

  constructor(maxSize = 100) {
    this.maxSize = maxSize;
  }

  record(resolution: ContextResolution): void {
    this.log.push({
      ...resolution,
      timestamp: Date.now()
    });

    // Prune old entries
    if (this.log.length > this.maxSize) {
      this.log.shift();
    }
  }

  getLast(n = 10): ContextResolution[] {
    return this.log.slice(-n);
  }

  getBySource(source: ResolutionSource): ContextResolution[] {
    return this.log.filter(r => r.source === source);
  }

  getByModel(model: string): ContextResolution[] {
    return this.log.filter(r => r.model === model);
  }

  // Debug helper: Why did we get this token count?
  explain(model: string): string {
    const resolutions = this.getByModel(model);
    if (resolutions.length === 0) {
      return `No resolution history for ${model}`;
    }

    const latest = resolutions[resolutions.length - 1];
    return `${model}: ${latest.tokens} tokens from ${latest.source} at ${new Date(latest.timestamp).toISOString()}`;
  }
}

// Global instance for the session
const globalAuditor = new ContextResolutionAuditor();

export function getContextAuditor(): ContextResolutionAuditor {
  return globalAuditor;
}
```

## Integration with Context Resolution

```typescript
// In resolveContextTokensForModel
function resolveContextTokensForModel(
  model: string,
  provider?: string,
  overrides?: ContextOverrides
): number {
  const auditor = getContextAuditor();

  // Check override first
  if (overrides?.maxTokens) {
    auditor.record({
      tokens: overrides.maxTokens,
      source: 'override',
      provider,
      model,
      details: { overrideKey: 'maxTokens' }
    });
    return overrides.maxTokens;
  }

  // Check configured provider window
  const providerWindow = getProviderWindow(provider, model);
  if (providerWindow) {
    auditor.record({
      tokens: providerWindow,
      source: 'configured-provider-window',
      provider,
      model
    });
    return providerWindow;
  }

  // Check cache with qualified key
  const cached = getCachedWindow(`${provider}:${model}`);
  if (cached) {
    auditor.record({
      tokens: cached,
      source: 'cache-qualified-key',
      provider,
      model,
      details: { cacheKey: `${provider}:${model}` }
    });
    return cached;
  }

  // Check cache with bare key
  const bareCached = getCachedWindow(model);
  if (bareCached) {
    auditor.record({
      tokens: bareCached,
      source: 'cache-bare-key',
      provider,
      model,
      details: { cacheKey: model }
    });
    return bareCached;
  }

  // Fallback
  const fallback = getDefaultWindow();
  auditor.record({
    tokens: fallback,
    source: 'fallback',
    provider,
    model
  });
  return fallback;
}
```

## Debugging Output

```typescript
// During development or with debug flag
function logContextDecisions(): void {
  const auditor = getContextAuditor();
  
  console.log('=== Context Resolution History ===');
  for (const resolution of auditor.getLast(20)) {
    console.log(
      `[${new Date(resolution.timestamp).toISOString()}] ` +
      `${resolution.model}: ${resolution.tokens} tokens ` +
      `(source: ${resolution.source})`
    );
  }
}
```

## Benefits

1. **Transparency**: Understand why context size was chosen
2. **Debugging**: Trace through fallback logic
3. **Optimization**: Identify over-reliance on fallbacks
4. **Testing**: Verify resolution logic in tests

## References

- Night Cycle: night_cycle_20250411_0031.md
- Related: Media Server Observability pattern
