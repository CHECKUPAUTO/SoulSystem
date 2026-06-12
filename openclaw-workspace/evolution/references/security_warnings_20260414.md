# Security Warnings (2026-04-14)

> Tracked from night_cycle_20260414_0130.md installation scan

## Active Warnings

| # | Warning | Severity | Action Required |
|:---|:---|:---|:---|
| 1 | Reverse proxy headers not trusted | ⚠️ Medium | Configure `trustProxy` in gateway config if behind reverse proxy |
| 2 | Control UI insecure auth toggle enabled (`allowInsecureAuth=true`) | 🔴 High | Disable in production; only enable for local debugging |
| 3 | Insecure/dangerous config flags enabled | 🔴 High | Audit and disable unnecessary dangerous flags |
| 4 | Potential multi-user setup detected | ⚠️ Medium | Verify intended configuration; ensure proper access controls |

## Security Hardening Priorities (from 02:00 cycle)

| Action | Priority | Status |
|:---|:---|:---|
| Fix Gateway WS 1006 closure | P0 | ⚠️ Blocks cron/tasks — needs `openclaw gateway restart` |
| Disable `allowInsecureAuth=true` | P0 | ⚠️ Active security risk |
| Configure `trustedProxies` | P1 | Needed for reverse proxy |
| Brain node authentication (9010-9015) | P2 | Currently no auth on ports |
| Brain node TLS | P3 | Currently HTTP only |

⚠️ **All items require manual review and approval before changes.** Do NOT auto-apply security configuration changes or gateway restarts.

## Context

- Gateway version: 2026.4.12 (stable, up to date)
- Gateway WS: `ws://127.0.0.1:18889/ws` — currently unreachable (1006 abnormal closure)
- Gateway PID: 1950521 (systemd enabled, running)
- Tailscale: OFF
- System: Debian, 125GB RAM, root access
- Channels: Telegram ✅, WhatsApp ✅
- Plugins: 51/100 loaded
- Skills: 23 workspace skills installed
- Agents: 15 agents, 31 active sessions, default glm-5.1:cloud (203k ctx)
- Gateway dashboard: `http://127.0.0.1:18890/`

## Additional Findings (03:00 / 03:30 cycles)

- **Gateway WS 1006 root cause identified**: Environment variable `OPENCLAW_GATEWAY_URL=ws://127.0.0.1:18889/ws` points to port 18889, but gateway listens on port 18890. This config mismatch explains the persistent 1006 closure.
- **Brain node authentication**: No auth on ports 9010-9015 — any local process can POST stimuli to any organ.
- **Brain node TLS**: All inter-organ communication is HTTP only (no encryption).
- **sl13-mod-evolve.py** running as Python (17.4M RAM) when Rust replacement (`night-cycle-engine`) is already compiled and available at `/mnt/nvme/soullink_brain/openevolve-rust/target/release/night-cycle-engine`.

⚠️ **All items require manual review and approval before changes.** Do NOT auto-apply security configuration changes, gateway restarts, or process kills.

## Source

- `night_cycle_20260414_0130.md` — Full installation scan (4 warnings)
- `night_cycle_20260414_0200.md` — Security hardening priorities with P0-P3 levels
- `night_cycle_20260414_0230.md` — Installation scan confirmation (4 warnings persist)
- `night_cycle_20260414_0300.md` — Gateway WS 1006 root cause: env var port mismatch (18889 vs 18890), brain node auth gap
- `night_cycle_20260414_0330.md` — Confirmed 4 warnings persist, sl13-mod-evolve.py replaceable

## Last Updated

2026-04-14T03:49:00+02:00 — Auto-apply cycle (03:00+03:30: WS 1006 root cause identified as port mismatch, brain node auth gap noted, sl13-mod-evolve.py replacement available)