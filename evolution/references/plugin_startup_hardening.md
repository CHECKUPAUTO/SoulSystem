# Plugin Startup Hardening

**Priority:** P2 (Medium)
**Source:** Night Cycle 2026-04-13 04:31, 04:05 (commits ad7f605a, d77360c0, fcee2683, 65259, 65429, 64780, 64786, 65459, 65427)
**Status:** Reference documentation
**Applies to:** OpenClaw plugin architecture, startup resilience

---

## Problem

Plugin startup remains fragile despite multiple fixes:
- Bundled peer resolution failures (#65365)
- Missing native dependency resolution
- Channel metadata loading instability
- Manifest activation complexity increasing

## Recent Fixes

| Commit | Issue | Fix |
|--------|-------|-----|
| `ad7f605a` | #65365 | Tolerate bundled peer resolution mismatches |
| `d77360c0` | — | Restore missing native runtime deps |
| `4503a43b` | — | Stabilize bundled channel metadata loading |
| `fcee2683` | #65620 | QA-lab: scenario-defined plugin runs |
| `65259` | #65259 | Narrow explicit provider loads from manifests |
| `65429` | #65429 | Narrow channel loads from manifests |
| `64780` | #64780 | Add manifest activation and setup descriptors |
| `64786` | #64786 | Prefer setup descriptors for setup lookup |
| `65459` | #65459 | Centralize manifest owner trust policy |
| `65427` | #65427 | Centralize WhatsApp account connection lifecycle |

## Manifest Activation Architecture (New)

```typescript
// Plugin manifest now includes activation descriptors
interface PluginManifest {
  // ... existing fields ...
  activation?: {
    mode: 'eager' | 'lazy' | 'on-demand';
    setup: SetupDescriptor[];
    dependencies: string[];
    trust: 'owner' | 'community' | 'untrusted';
  };
}
```

This is a significant shift: plugins can now declare whether they should be loaded eagerly (at startup), lazily (on first use), or on-demand (when explicitly invoked).

## Recommendations

1. **Smoke tests for plugin load with missing peer deps** — Verify graceful degradation
2. **`--strict` vs `--tolerant` modes** — Strict mode fails on missing peers, tolerant mode degrades
3. **QA-lab scenario runs** — Cover peer dep mismatch, missing native deps, trust policy edge cases
4. **Startup dependency graph** — Build and visualize plugin load order from manifest descriptors
5. **Health check per plugin** — After activation, verify plugin health before marking as ready

## Related References

- `deferred_activation_pattern.md` — Noop placeholder pattern for startup
- `bundled_plugin_resolution.md` — Plugin peer resolution tolerance
- `plugin_peer_resolution_fragility.md` — CI validation proposal
- `narrow_surface_pattern.md` — Minimal API surface area