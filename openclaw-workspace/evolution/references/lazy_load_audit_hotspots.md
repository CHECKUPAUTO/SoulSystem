# Lazy-Load Import Audit Hotspots

**Priority:** P2 (Medium)
**Source:** Night Cycle 2026-04-13 04:31, 04:05 (commits 5b2ae491, 5d9a04d4, 03d042d2)
**Status:** Reference documentation
**Applies to:** OpenClaw gateway cold-start optimization

---

## Problem

Three recent commits show a concerted lazy-loading effort with high impact-to-size ratio:
- `5b2ae491` — Extract `attempt-execution.helpers.ts` (169 new lines, cuts test boot time)
- `5d9a04d4` — Lazy-load session store helpers (15 insertions, defers heavy imports)
- `03d042d2` — Mock hot agents import tests (reduces test fixture weight)

The 15-LOC session store lazy-load is particularly notable — high impact, minimal code change.

## Pattern: Lazy Import at First Use

```typescript
// Before: eager import on module load
import { heavyHelper } from './heavy-module';

// After: lazy import at first call
let _heavyHelper: typeof import('./heavy-module')['heavyHelper'];
function getHeavyHelper() {
  if (!_heavyHelper) {
    ({ heavyHelper } = await import('./heavy-module'));
  }
  return _heavyHelper;
}
```

## Known Hotspot Candidates

Based on the commit history and barrel bypass campaign:

| Module | Current State | Lazy-Load Candidate | Estimated Impact |
|--------|---------------|---------------------|-----------------|
| `session-store.ts` | ✅ Already lazy-loaded | — | — |
| `attempt-execution.helpers.ts` | ✅ Already extracted | — | — |
| `bash-tools.exec-runtime.ts` | Eager import | High | Large dependency tree |
| `agents/auth` fixtures | Eager import in tests | High | Heavy test fixture |
| `bundled plugin contracts` | Eager barrel import | Medium | Part of barrel bypass |
| `channel-metadata` | Deferred (7591d01) | ✅ Already deferred | — |
| `channel-presence` | Deferred (2d6519d) | Medium | Part of barrel bypass |

## Cold-Start Audit Protocol

1. **Profile gateway boot** — Measure import resolution time per module
2. **Identify heavy imports** — Modules >50ms resolution time
3. **Apply lazy-load** — Defer to first use where possible
4. **Verify** — Confirm sub-2s cold start for sidecar activation

## Target Metric

- **Current:** ~3-5s cold start (estimated from barrel bypass campaign progress)
- **Target:** Sub-2s for sidecar activation
- **Stretch:** Sub-1s for pure function resolution

## Related References

- `barrel_bypass_campaign_tracker.md` — Barrel elimination progress
- `barrel_bypassing_guide.md` — How to bypass barrel imports
- `startup_benchmark_harness.md` — CI benchmark for startup time
- `narrow_surface_pattern.md` — Minimal API surface area principles
- `lazy_loading_pattern.md` — LazyModule<T> wrapper pattern