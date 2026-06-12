# Security Hardening Notes (2026-04-14 00:00)

> Extracted from night_cycle_20260414_0000.md — Section 5.4

## Current Security Observations

1. **Gateway auth token**: Token unchanged since initial setup — needs rotation
2. **Inter-organ communication**: No mTLS between organs, currently HTTP without TLS on loopback (ports 9010-9015)
3. **Reflex organ**: Should provide first-line rate limiting defense once implemented
4. **Audit logging**: All organ state mutations need audit trail

## Proposed Security Actions

| Priority | Action | Effort | Risk | Auto-Apply? |
|:---|:---|:---|:---|:---|
| 🔴 P0 | Gateway auth token rotation | 1 day | Medium — stale token | ❌ Manual |
| 🔴 P1 | Add mTLS between organ-to-organ communication | 2-3 days | High — unauthenticated access | ❌ Manual |
| 🟠 P1 | Rate limiting on reflex organ (first-line defense) | 1-2 days | Medium — no rate limiting | ❌ Core code |
| 🟡 P2 | Audit logging on all organ state mutations | 2-3 days | Low — no audit trail | ❌ Core code |

## ⚠️ Auto-Apply Decision

All security actions are **security-sensitive** and require **manual review and approval** before implementation. Documented as reference only — not auto-applied.

## Source

- `night_cycle_20260414_0000.md`

## Last Updated

2026-04-14T00:12:00+02:00 — Auto-apply cycle