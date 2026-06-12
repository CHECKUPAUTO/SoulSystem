# Auth Pattern Audit — Implicit Selection Risks

**Date:** 2026-04-13  
**Source:** Night Cycle Report (00:45)  
**Status:** Proposal  
**Priority:** P1 (security-adjacent)  
**Bug Reference:** Issue #64160 — Device pairing wrong requester (implicit latest-device approval)  

## Problem

The device pairing bug (#64160) revealed that implicit "latest device" approval could pair the wrong requester. This suggests similar implicit-selection patterns may exist in other approval/auth flows.

## Pattern: Explicit Target Validation

**Before (vulnerable):**
```typescript
async function approveRequest(requestId?: string) {
  // Implicit: use latest pending request if no ID provided
  const request = requestId 
    ? await findRequest(requestId)
    : await findLatestPendingRequest();
  // WRONG: might approve wrong device
  return approve(request);
}
```

**After (secure):**
```typescript
async function approveRequest(requestId: string) {
  // Explicit: ALWAYS require a specific request ID
  const request = await findRequest(requestId);
  if (!request) throw new Error('Request not found');
  return approve(request);
}
```

## Audit Checklist

Search all `approve*` and `pair*` code paths for:

1. **Optional ID parameters** — any `id?` or `requestId?` that defaults to "latest"
2. **Implicit selection** — `findFirst`, `findLatest`, array `[0]` without explicit user intent
3. **Race conditions** — multiple pending requests where wrong one could be selected
4. **Missing validation** — approving without verifying the target matches expected device/user

## Known Instances

- Device pairing: Fixed in #64160
- Signal approval: Check `signal-approval` paths for similar patterns
- Plugin permissions: Check `plugin-approve` for implicit selection

## Related Patterns

- `security_audit_patterns.md` — Existing security audit reference
- `explicit_seams_pattern.md` — Making implicit boundaries explicit