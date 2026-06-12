# System Status

_Synthesis page — current state of all systems._

**Generated**: 2026-04-13 19:25 UTC

## Health

| System | Status | Notes |
|--------|--------|-------|
| OpenClaw Gateway | 🟢 Running | Port 18890, loopback |
| Ollama | 🟢 Running | localhost:11434 |
| SoulLink V13 | 🟢 6/6 Online | All nodes HTTP 200 |
| Telegram | 🟢 Connected | Bot token active |
| WhatsApp | 🟢 Connected | Self-chat mode |
| n8n | 🟡 Unknown | Not checked recently |
| Exec tool | 🔴 Broken | pi-tools module missing |

## Open Issues

| Priority | Issue | Status | Days Stale |
|----------|-------|--------|-----------|
| P0 | Barrel file elimination | Approved, not started | 2+ |
| P0 | pi-tools module missing | Unresolved | 0 |
| P1 | CDP duplicate tabs (#13851) | Backlog | 5+ |
| P1 | PR #63680 (CVSS 8.5 security) | Unmerged | 5+ |
| P1 | Issue #63686 (Discord ACP) | Uninvestigated | 5+ |
| P2 | SSH port 2222 still open | Manual fix needed | — |
| P2 | Config world-readable (644) | chmod 600 needed | — |
| P2 | Stale plugins in config | Config edit needed | — |

## Recent Completions (Today)
- ✅ Workspace cleanup (602MB freed, 122 broken symlinks)
- ✅ MemPalace v3.1.0 installed (59,619 files indexed, 59 rooms)
- ✅ Night Cycle auto-apply (6 reports, 2 docs applied)
- ✅ LLM-Wiki pattern implemented

## Metrics
- Sessions: 41 active
- Memory: 22 files, 60 chunks
- Plugins: ollama ✅, memory-core ✅, openclaw-web-search ⚠️ (unpinned)
- Tasks: 5 active, 345 issues

## See Also
- [openclaw](../entities/openclaw.md)
- [soullink](../entities/soullink.md)