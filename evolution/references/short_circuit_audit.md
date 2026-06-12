# Short-Circuit Audit Pattern

**Source:** Night cycle reports 2026-04-13 (0433, 0418)
**Status:** Reference documentation
**Priority:** P2

---

## Pattern

The short-circuit optimization replaces expensive runtime lookups with early-return checks when static or pre-computed data can determine the answer.

### Discovered Instances (April 2026)

| Commit | Short-Circuit | Savings |
|--------|---------------|---------|
| `be9b70c815` | Exact reply suppression targets | Skip channel lookup when target is exact match |
| `e2d93fb5bc` | Static doctor channel capabilities | Pre-computed capability set, no runtime plugin lookup |
| `2cfd1459ef` | Command body normalization | Split normalization into fast-path (identity) and full-path |
| `8fb482268f` | Queue settings import | Direct import bypasses barrel |
| `7591d01` | Bundled channel metadata | Deferred to first access |
| `2d6519d` | Bundled channel presence | Deferred to first access |

### Pattern Template

```typescript
// BEFORE: Expensive lookup every time
function getCapability(channel: string): Cap {
  return pluginRegistry.get(channel)?.capabilities // loads all plugins
}

// AFTER: Short-circuit with static data
const STATIC_CAPS: Record<string, Cap> = { /* pre-computed */ }
function getCapability(channel: string): Cap | undefined {
  return STATIC_CAPS[channel] // O(1), no plugin load
}
```

### Candidates for Short-Circuit (from reports)

- **Session history reads** — many reads only need last N messages, load full history only when needed
- **Channel plugin capability resolution** — already partially done, extend to all hot-path lookups
- **Session store initialization** — defer heavy imports until first session access
- **Plugin metadata resolution** — cache resolved metadata at startup, serve from cache

### Impact Estimate

Each short-circuit reduces hot-path latency by ~5-15%. Cumulative effect of 6+ short-circuits: estimated 30-50% reduction in gateway message processing latency on cold starts.

---

## Cross-References

- `performance_optimization_patterns.md` — broader perf patterns
- `barrel_bypassing_guide.md` — related import optimization
- `static_capability_generation.md` — compile-time capability maps