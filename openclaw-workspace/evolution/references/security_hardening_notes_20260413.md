# Security Hardening Notes (2026-04-13 22:54)

> Extracted from night_cycle_20260413_2254.md — Section E

## Current Security Observations

1. **V13 Module OAuth**: Decision engine, evolve, and reinforcement critic run as separate services but share an Ollama API key. **Action**: Rotate and isolate keys per service.
2. **Gateway TLS**: Currently loopback-only, but if remote access is needed, TLS termination should be in Rust, not Node.js.
3. **Node Auth**: Brain nodes accept unauthenticated requests on ports 9010-9015. **Action**: Add HMAC-based node authentication.

## Proposed Security Actions

| Priority | Action | Effort | Risk |
|:---|:---|:---|:---|
| 🔴 P0 | Isolate V13 service API keys | 1h | Medium — shared key exposure |
| 🟠 P1 | Add HMAC auth to brain nodes | 2-3 days | High — unauthenticated access |
| 🟡 P2 | TLS termination in Rust (future) | 1 week | Low — loopback only currently |

## ⚠️ Auto-Apply Decision

These are **security-sensitive** and require **manual review and approval** before implementation. Documented as reference only — not auto-applied.

## Source

- `night_cycle_20260413_2254.md`

## Last Updated

2026-04-13T23:11:00+02:00 — Auto-apply cycle