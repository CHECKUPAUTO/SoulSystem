# OAuth Token Scope Integrity Test Pattern

**Created:** 2026-04-12
**Source:** Night cycle 2026-04-12 23:16, based on Codex OAuth scope fix (58708e6f)
**Priority:** High — security-critical, affects ACP harness

## Problem

OAuth scopes can be silently dropped during token refresh, causing authentication failures that are hard to diagnose. The Codex OAuth scope preservation bug (fixed in 58708e6f) demonstrated this class of issue.

## Pattern: Scope Integrity Regression Tests

```typescript
describe('OAuth scope integrity', () => {
  it('preserves all scopes during token refresh', async () => {
    const originalScopes = ['openid', 'profile', 'codex:read', 'codex:write'];
    const refreshedToken = await refreshOAuthToken(token, originalScopes);
    expect(refreshedToken.scopes).toEqual(originalScopes);
  });

  it('detects scope subset after refresh', async () => {
    const refreshedScopes = ['openid']; // missing scopes!
    const diff = scopeDiff(originalScopes, refreshedScopes);
    expect(diff.missing).toContain('codex:read');
    // Should log warning and attempt re-auth
  });

  it('triggers re-authentication on scope mismatch', async () => {
    // If refresh returns fewer scopes, system must re-authenticate
  });
});
```

## Implementation Notes

- Add `scopeDiff()` utility to compare before/after scope sets
- Log scope diffs at WARN level for operational visibility
- Integration test across all OAuth providers (Codex, Google, Feishu, etc.)
- Cron regression harness already being hardened (6883273)

## Cross-References

- `oauth_scope_chain_pattern.md` — generalized scope assertion middleware
- `codex_harness_integration_guide.md` — Codex integration context