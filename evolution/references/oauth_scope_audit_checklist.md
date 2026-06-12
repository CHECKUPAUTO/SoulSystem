# OAuth Scope Audit Checklist

**Source:** OpenEvolve Night Cycle Reports 2026-04-11 (2349, 2346, 2115)  
**Priority:** P0 Critical  
**Related Commit:** 58708e6f88 (Codex OAuth scope preservation fix)

## Overview

Commit 58708e6f88 addressed a critical OAuth scope regression where requested scopes were being dropped during token refresh/exchange. This checklist ensures comprehensive audit of all OAuth flows.

## Critical Findings

- **Issue:** OAuth scopes dropped during token refresh
- **Impact:** Auth flow fragility, potential security downgrade
- **Fix:** Proper scope preservation in 58708e6f88

## Audit Checklist

### 1. Code Review - All OAuth Flows

- [ ] **Codex OAuth Flow** (`src/auth/codex-oauth.ts`)
  - Verify `preserveScopes: true` or equivalent
  - Check scope parameter forwarding
  - Validate scope validation on token exchange

- [ ] **Telegram Bot OAuth**
  - Scope preservation in token refresh
  - Token storage includes scope metadata

- [ ] **Discord OAuth**
  - Scope preservation in token refresh
  - Token storage includes scope metadata

- [ ] **Generic OAuth Handler**
  - `src/auth/oauth-handler.ts`
  - Check base OAuth implementation

### 2. Test Coverage

- [ ] **Regression Test Required**
  ```typescript
  // auth/oauth-scope-preservation.test.ts
  describe('OAuth Scope Preservation', () => {
    it('preserves scopes through token refresh', async () => {
      const originalScopes = ['read', 'write'];
      const token = await exchangeCode('auth-code', originalScopes);
      const refreshed = await refreshToken(token.refreshToken);
      
      expect(refreshed.scopes).toEqual(originalScopes);
    });
  });
  ```

- [ ] **E2E OAuth Flow Test**
  - Full flow from auth request to token refresh
  - Verify scope consistency at each step

### 3. Configuration Audit

- [ ] **Environment Variables**
  - `OAUTH_SCOPE_PRESERVATION` flag documented
  - Default behavior documented

- [ ] **Token Storage**
  - Scopes stored alongside tokens
  - Schema migration if needed

### 4. Security Review

- [ ] **Scope Escalation Prevention**
  - Verify refresh cannot expand scopes
  - Validate scope subset check

- [ ] **Token Introspection**
  - `/oauth/introspect` returns correct scopes
  - Scope validation on protected endpoints

## Implementation Template

```typescript
// src/auth/oauth-scope-utils.ts

export interface TokenResponse {
  accessToken: string;
  refreshToken: string;
  expiresIn: number;
  scopes: string[];
}

/**
 * Preserves original scopes during token refresh
 */
export function preserveScopes(
  originalScopes: string[],
  newScopes: string[] | undefined
): string[] {
  // Refresh tokens should NOT expand scopes
  if (!newScopes) {
    return originalScopes;
  }
  
  // Verify new scopes are subset of original
  const expanded = newScopes.filter(s => !originalScopes.includes(s));
  if (expanded.length > 0) {
    throw new OAuthError(
      'scope_escalation',
      `Scope escalation detected: ${expanded.join(', ')}`
    );
  }
  
  return newScopes;
}

/**
 * Validates scope preservation in token exchange
 */
export function validateScopePreservation(
  requestScopes: string[],
  responseScopes: string[]
): boolean {
  // Response scopes should match or be subset of request
  const missing = requestScopes.filter(s => !responseScopes.includes(s));
  if (missing.length > 0) {
    console.warn(`Scopes dropped: ${missing.join(', ')}`);
    return false;
  }
  return true;
}
```

## Testing Strategy

| Test Type | Coverage | Priority |
|-----------|----------|----------|
| Unit - preserveScopes() | Required | P0 |
| Unit - validateScopePreservation() | Required | P0 |
| Integration - Codex flow | Required | P0 |
| E2E - Full OAuth cycle | Recommended | P1 |
| Security - Scope escalation | Required | P0 |

## Documentation Updates

- [ ] Update `docs/security/oauth-scopes.md`
- [ ] Add to `CONTRIBUTING.md` security section
- [ ] Code comments in OAuth handlers
- [ ] Runbook for scope-related incidents

## Follow-up Actions

1. **Immediate:** Add regression test for commit 58708e6f88
2. **This Week:** Audit all OAuth flows with this checklist
3. **Next Sprint:** Implement `preserveScopes()` utility
4. **Ongoing:** Include scope preservation in OAuth PR reviews

## References

- Night Cycle Reports: night_cycle_20260411_2349.md, night_cycle_20260411_2346.md, night_cycle_20260411_2115.md
- Commit: 58708e6f88 (fix: preserve Codex OAuth scopes)
- Related: `docs/security/oauth-scopes.md`
