# Codex OAuth Token Lifecycle

**Priority:** High (from 0230 report, S3)  
**Status:** Proposal  
**Created:** 2026-04-13  
**Source:** OpenEvolve Night Cycle 0230

## Problem

The OAuth scope preservation fix (`58708e6f88`) added +287 lines of test code, suggesting complex state management. Token lifecycle (refresh, revoke, scope preservation) is scattered across the provider module.

## Proposal: OAuthTokenManager

```typescript
// codex-scope-manager.ts
class OAuthTokenManager {
  private tokens: Map<string, ScopedToken>;
  
  async acquire(scopes: string[]): Promise<ScopedToken>;
  async refresh(token: ScopedToken): Promise<ScopedToken>;
  async revoke(token: ScopedToken): Promise<void>;
  
  // Scope integrity validation
  validateScopePreservation(original: string[], current: string[]): ScopeDiff;
}
```

## Key Principles

1. **Scope identity validation** — Never lose scopes during refresh
2. **Clear refresh/revoke semantics** — Explicit lifecycle methods
3. **Single responsibility** — Token management separated from provider logic
4. **Observable** — Scope changes logged and auditable

## Related References

- `oauth_scope_preservation.md` — Scope round-trip regression test pattern
- `oauth_scope_integrity_test_pattern.md` — Scope integrity testing
- `codex_harness_circuit_breaker.md` — Circuit breaker for Codex harness