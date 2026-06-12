# Perf Budget CI Proposal

**Created:** 2026-04-13 (Night Cycle 00:30)
**Source:** OpenEvolve Night Cycle Report 2026-04-13 00:30
**Status:** Proposal
**Priority:** P2

## Context

The barrel-avoidance campaign has reduced cold-start import costs significantly. But there's no automated enforcement — regressions could be reintroduced.

Recent import depth reductions:

| Before | After | Impact |
|--------|-------|--------|
| Import full plugin registry | Direct import of needed function | -200ms cold start |
| Import channel barrel | Import specific module | -150ms per channel |
| Import session store barrel | Direct key access | -80ms session init |
| Import model catalog barrel | Static lookup table | -120ms model resolution |

## Proposal: `perf-budget.json`

```json
{
  "$schema": "perf-budget-schema.json",
  "modules": {
    "src/gateway/session/store.ts": {
      "maxImportDepth": 3,
      "maxDirectDependencies": 8,
      "maxBundleSizeKb": 50
    },
    "src/channels/plugins/registry.ts": {
      "maxImportDepth": 4,
      "maxDirectDependencies": 12
    },
    "src/plugins/runtime/runtime.ts": {
      "maxImportDepth": 3,
      "maxDirectDependencies": 10
    }
  },
  "defaults": {
    "maxImportDepth": 5,
    "maxDirectDependencies": 15,
    "maxBundleSizeKb": 100
  },
  "hotPaths": [
    "src/gateway/**",
    "src/channels/**/registry.ts",
    "src/plugins/runtime/**"
  ]
}
```

### CI Integration

```yaml
# .github/workflows/perf-budget.yml
- name: Import Depth Check
  run: node scripts/check-import-depths.js --budget perf-budget.json
```

### Script

```javascript
// scripts/check-import-depths.js
// Walks import graph, measures depth per module, flags violations
// Output: table of modules exceeding budget with current vs max depth
```

## References

- Barrel bypassing guide: `evolution/references/barrel_bypassing_guide.md`
- Performance optimization patterns: `evolution/references/performance_optimization_patterns.md`