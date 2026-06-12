# Auth Pattern Audit: Implicit-Selection Vulnerability

**Created:** 2026-04-13 (Night Cycle auto-apply)
**Priority:** P1
**Source Reports:** night_cycle_20260413_0045.md
**Status:** Proposal — requires manual security audit

## Problem

Coy Geek's fix (#64160) revealed that implicit device approval could pair the wrong requester. This suggests other implicit-selection auth flows may have similar vulnerabilities.

**Pattern:** Any code path that selects "the latest device" or "the first pending request" without explicit target validation is at risk.

## Audit Scope

All `approve*` and `pair*` code paths should be reviewed for:

1. **Implicit selection** — Does the code pick a device/request by position rather than explicit ID?
2. **Missing requester validation** — Does the approval path verify WHO is approving, not just THAT approval happened?
3. **Race conditions** — Could two simultaneous requests cause the wrong one to be approved?

### Specific Targets

- `src/channels/plugins/approvals.ts` — approval flows
- `src/device-pairing/` — device pairing flows
- `src/plugins/runtime/` — plugin approval hooks
- Any `getContext()` calls that don't validate target

## Recommendation

Create a type guard pattern for approval flows:

```typescript
// Instead of:
const device = pendingDevices[0]; // IMPLICIT - vulnerable

// Use:
const device = pendingDevices.find(d => d.id === targetDeviceId);
if (!device) throw new ApprovalError('INVALID_TARGET');
// EXPLICIT - validated
```

## Related References

- `evolution/references/security_audit_patterns.md`
- `evolution/references/oauth_scope_preservation_guide.md`
- Issue #64160: Device pairing wrong requester fix