# OAuth Scope Chain Pattern

**Created:** 2026-04-12
**Source:** Codex OAuth scope fix (commit 58708e6f), generalized pattern

## Problem

OAuth scopes can be silently dropped during authentication flows, particularly:
- Token refresh operations
- Provider-specific auth pipelines
- Multi-step OAuth handshakes (e.g., Codex)

## Pattern: Scope Assertion Middleware

```
1. Assert expected scopes BEFORE auth flow begins
2. Log scope diff after each token operation (refresh, exchange)
3. Fail fast if scopes are missing post-auth
4. Integration test: verify scope preservation across all providers
```

## Implementation Checklist

- [ ] Scope snapshot at auth start
- [ ] Scope diff logging (before/after refresh)
- [ ] Scope assertion middleware for all OAuth flows
- [ ] Integration tests for scope preservation per provider

## References

- Codex fix commit: 58708e6f
- Codex harness integration guide
