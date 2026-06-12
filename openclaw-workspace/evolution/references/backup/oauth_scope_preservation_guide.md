# OAuth Scope Preservation Guide
**Generated:** 2026-04-11 23:53 UTC
**Source:** night_cycle_20260411_2346.md, night_cycle_20260411_2349.md (commit 58708f6)
**Status:** ⚠️ Critical Fix Documented

---

## Overview

Commit `58708f6f88` fixed a critical OAuth scope preservation bug for Codex integration. This document describes the issue, the fix, and preventive measures.

---

## The Bug

**Issue:** OAuth scopes were being dropped during token refresh or exchange.

**Impact:** 
- Codex OAuth flows lost required scopes
- Agentic execution contract integrity compromised
- Potential security degradation

**Root Cause:** Scopes not explicitly preserved during token operations.

---

## The Fix

### Before (Buggy)

```typescript
async function refreshToken(token: Token): Promise<Token> {
  const response = await oauthClient.refresh(token.refreshToken);
  return {
    accessToken: response.access_token,
    refreshToken: response.refresh_token || token.refreshToken,
    // Missing: scopes not carried over
    expiresAt: Date.now() + response.expires_in * 1000,
  };
}
```

### After (Fixed)

```typescript
async function refreshToken(token: Token): Promise<Token> {
  const response = await oauthClient.refresh(token.refreshToken);
  return {
    accessToken: response.access_token,
    refreshToken: response.refresh_token || token.refreshToken,
    scopes: response.scope?.split(' ') ?? token.scopes, // Preserve scopes
    expiresAt: Date.now() + response.expires_in * 1000,
  };
}
```

---

## Scope Preservation Checklist

When implementing OAuth flows:

- [ ] Preserve original scopes during token refresh
- [ ] Handle scope reduction from OAuth server
- [ ] Validate required scopes after exchange
- [ ] Log scope changes for audit
- [ ] Test with partial scope grants

---

## Regression Test Template

```typescript
describe('OAuth Scope Preservation', () => {
  it('should preserve scopes on token refresh', async () => {
    const originalToken = createToken({ 
      scopes: ['codex:execute', 'codex:read'] 
    });
    
    mockOAuthResponse({ access_token: 'new-token' }); // No scope in response
    
    const newToken = await refreshToken(originalToken);
    
    expect(newToken.scopes).toEqual(['codex:execute', 'codex:read']);
  });

  it('should update scopes when server returns new scope list', async () => {
    const originalToken = createToken({ 
      scopes: ['codex:execute', 'codex:read', 'codex:admin'] 
    });
    
    mockOAuthResponse({ 
      access_token: 'new-token',
      scope: 'codex:execute codex:read' // Reduced scope
    });
    
    const newToken = await refreshToken(originalToken);
    
    // Server scope takes precedence
    expect(newToken.scopes).toEqual(['codex:execute', 'codex:read']);
  });
});
```

---

## Audit Recommendations

1. **Review all OAuth flows** for scope handling
2. **Add regression tests** to prevent future regressions
3. **Document scope requirements** for each integration
4. **Monitor scope changes** in production logs

---

## Related Commits

- `58708f6f88` - fix: preserve Codex OAuth scopes (#64713)

---

*Auto-generated from OpenEvolve Night Cycle analysis*
