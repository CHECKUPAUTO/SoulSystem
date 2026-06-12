# Critical Bug Tracker — 2026-04-13

**Created:** 2026-04-13 (Night Cycle auto-apply)
**Source Reports:** night_cycle_20260413_0052.md
**Status:** Reference — tracks open bugs from night cycle analysis

## Open Bugs (from Night Cycle #0052)

### P0 — Critical

| # | Issue | Impact | Proposed Fix |
|---|-------|--------|--------------|
| 65582 | ENOENT mkdir `/home/node` on macOS (no Docker) | Blocks native macOS users | Check `process.env.HOME` and `os.homedir()` before mkdir; add startup preflight |
| 65581 | Duplicate Discord messages on every response | User-facing regression | Add message dedup with `Set<string>` of recent message IDs (TTL 5s) at channel dispatch |
| 65566 | Streaming partialParse JSON errors crash agent runs | Crashes | Add try/catch wrapper around JSON.parse of streaming chunks; buffer incomplete JSON and retry |

### P1 — High

| # | Issue | Impact | Proposed Fix |
|---|-------|--------|--------------|
| 65576 | Cron silently disables LLM idle watchdog; hung providers block failover | Reliability cascade | Decouple watchdog from cron; separate timer that resets on LLM activity |
| 65571 | Browser CDP fails on Linux Elementary OS | Platform coverage | Add platform-specific fallback for CDP |
| 65568 | Discord-bound persistent Codex ACP session can't resume | ACP reliability | Review session resume logic for Discord binding |

### P2 — Medium

| # | Issue | Impact | Proposed Fix |
|---|-------|--------|--------------|
| 65580 | macOS Gmail/Chrome runs need Apple Events JS permission | UX friction | Better error messages and permission prompts |

## Fix Patterns

### Stream Parser Resilience (for #65566)
```typescript
function safePartialParse(chunk: string, buffer: string): { parsed: any; remaining: string } {
  const combined = buffer + chunk;
  try {
    return { parsed: JSON.parse(combined), remaining: '' };
  } catch {
    // Buffer incomplete JSON for next chunk
    return { parsed: null, remaining: combined };
  }
}
```

### Discord Dedup Guard (for #65581)
```typescript
const recentMessageIds = new Map<string, number>(); // id → timestamp
const DEDUP_TTL_MS = 5000;

function isDuplicate(messageId: string): boolean {
  const now = Date.now();
  const lastSeen = recentMessageIds.get(messageId);
  if (lastSeen && (now - lastSeen) < DEDUP_TTL_MS) return true;
  recentMessageIds.set(messageId, now);
  // Evict expired entries
  for (const [id, ts] of recentMessageIds) {
    if (now - ts > DEDUP_TTL_MS) recentMessageIds.delete(id);
  }
  return false;
}
```

### macOS Home Directory Fallback (for #65582)
```typescript
function ensureDirectory(path: string): void {
  const targetDir = path === '/home/node'
    ? (process.env.HOME || os.homedir())
    : path;
  fs.mkdirSync(targetDir, { recursive: true });
}
```

## Related References

- `evolution/references/circuit_breaker_pattern.md` (for watchdog decoupling)
- `evolution/references/security_audit_patterns.md`

## Update 2026-04-13 02:11

New issues from Night Cycle 01:47:

### New P1 Issues

| # | Issue | Impact | Proposed Fix |
|---|-------|--------|--------------|
| #65219 | GPT-5.4 runtime completion rollup — incomplete turn handling | Agent reliability | See `incomplete_turn_handling.md` and `gpt54_runtime_rollup.md` |
| #ad7f605 | Plugin peer resolution fragility — two hotfixes in one cycle | Build stability | See `plugin_peer_resolution_fragility.md` |
| #65512 | WhatsApp group reactions sent to wrong recipient | UX correctness | Add target participant support |
| #65573 | TTS tagged syntax guidance mismatch | UX clarity | Correct doc to match runtime behavior |
| #65148 | OpenAI heartbeat overlay guidance insufficient | Session reliability | Strengthen documentation |
| #65491 | CLI audio provider auto-detection | UX improvement | See `audio_provider_env_detection.md` |

### New P2 Issues

| # | Issue | Impact | Proposed Fix |
|---|-------|--------|--------------|
| #65552 | Release workflow separation | CI reliability | Add smoke test for new workflow |
| Test fixtures | Lazy-loading optimization | Test startup perf | Quantify savings, apply pattern broadly |

### Cross-References Added
- `gpt54_runtime_rollup.md` — GPT-5.4 agent completion handling
- `plugin_peer_resolution_fragility.md` — Plugin bundling issues
- `barrel_bypass_campaign_tracker.md` — Barrel elimination progress
- `audio_provider_env_detection.md` — CLI provider auto-detection