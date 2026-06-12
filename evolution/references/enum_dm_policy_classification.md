# Enum-Based DM Policy Classification

**Priority:** P2 (from 0315 report)  
**Status:** Proposal  
**Created:** 2026-04-13  
**Source:** night_cycle_20260413_0315.md  

---

## Problem

`classifyChannelWarningSeverity()` in `audit-channel.ts` uses string matching to classify DM policies:

```typescript
// Current: fragile string matching
if (policy.includes("dms: open")) { /* ... */ }
if (policy.includes("dms: restricted")) { /* ... */ }
```

**Issues:**
- **Fragile** — Any policy string change breaks classification silently
- **Locale-dependent** — String matching can't handle i18n
- **Not exhaustively checked** — Missing policies silently fall through
- **Hard to extend** — New policies require adding new `if` branches

## Proposed Solution: DmPolicy Enum

```typescript
enum DmPolicy {
  Open = "dms:open",
  Restricted = "dms:restricted",
  Closed = "dms:closed",
  // extensible
}

function classifyChannelWarningSeverity(policy: DmPolicy): Severity {
  switch (policy) {
    case DmPolicy.Open: return Severity.Info;
    case DmPolicy.Restricted: return Severity.Warning;
    case DmPolicy.Closed: return Severity.Critical;
  }
}
```

## Benefits

- **Exhaustive** — TypeScript enforces all cases handled
- **Type-safe** — Can't pass arbitrary strings
- **Extensible** — New policies are enum members, compiler catches unhandled cases
- **No locale issues** — Enum values, not display strings

## Related References

- `security_audit_patterns.md` — Security hardening patterns
- `explicit_seams_pattern.md` — Minimal API surface area principles

## Status Tracking

- [ ] Upstream: `audit-channel.ts` currently uses string matching
- [ ] Proposal: Introduce `DmPolicy` enum
- [ ] Proposal: Add exhaustive switch pattern in severity classifier