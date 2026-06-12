# OAuth Scope Preservation and Drift Detection

**Date:** 2026-04-13  
**Source:** Night Cycle Reports (00:48, 01:00)  
**Status:** Proposal  
**Priority:** P2  

## Problem

Codex OAuth scopes were being lost across token refresh cycles (issue #64713, commit `58708e6`). While fixed, there's no early detection when auth providers change available scopes on refresh.

## Pattern: Scope Identity Validation

```typescript
interface TokenWithScopes {
  accessToken: string;
  scopes: string[];
}

function validateScopePreservation(
  original: TokenWithScopes, 
  refreshed: TokenWithScopes,
  provider: string
): void {
  const originalSet = new Set(original.scopes);
  const refreshedSet = new Set(refreshed.scopes);
  
  const gained = refreshed.scopes.filter(s => !originalSet.has(s));
  const lost = original.scopes.filter(s => !refreshedSet.has(s));
  
  if (lost.length > 0) {
    logger.warn(`OAuth scope drift detected for ${provider}`, {
      lost,
      gained,
      action: 'scopes_lost_on_refresh'
    });
  }
  
  if (gained.length > 0) {
    logger.info(`OAuth scopes gained on refresh for ${provider}`, { gained });
  }
}
```

## Round-Trip Regression Test

```typescript
describe('OAuth scope identity', () => {
  it('preserves scopes through JSON serialization round-trip', () => {
    const original = { accessToken: 'x', scopes: ['read', 'write', 'admin'] };
    const serialized = JSON.parse(JSON.stringify(original));
    expect(serialized.scopes).toEqual(original.scopes);
    expect(serialized.scopes).not.toBe(original.scopes); // Deep equality, not reference
  });
});
```

## Guidelines

1. **Always compare scope sets** on token refresh — log warnings for drift
2. **Never silently drop scopes** — if refreshed token has fewer scopes, that's a signal
3. **Add serialization round-trip tests** for any auth token types
4. **Alert on scope loss** — this often indicates provider-side permission changes

## Upstream Tracking

- Commit `58708e6f`: Preserve Codex OAuth scopes (#64713)