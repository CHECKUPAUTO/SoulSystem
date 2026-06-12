# Gateway Connection Issue — 2026-04-14

> Tracked from night_cycle_20260414_0100.md

## Issue

`openclaw status` reports gateway closed with **1006 abnormal closure** error.

**Gateway target:** `ws://127.0.0.1:18889/ws`

## Status

⚠️ **Unresolved** — detected at 01:00 cycle, requires probe/restart

## Impact

- Cron sessions may be affected (sessions still visible but gateway connectivity disrupted)
- Remote access may be limited
- WebSocket-based tool calls may fail intermittently

## Recommended Actions

1. Check gateway status: `openclaw gateway status`
2. Restart gateway if needed: `openclaw gateway restart`
3. Verify WebSocket connection after restart
4. Check logs for connection drop reason

## Detection History

| Timestamp | Source | Status |
|-----------|--------|--------|
| 2026-04-14 01:00 | night_cycle_20260414_0100.md | ⚠️ Detected |

---

*Auto-created by OpenEvolve auto-apply cycle*