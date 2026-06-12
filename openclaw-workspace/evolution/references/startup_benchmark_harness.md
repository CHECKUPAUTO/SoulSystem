# Startup Benchmark Harness

**Priority:** I2 (from 0216 report)  
**Status:** Proposal  
**Created:** 2026-04-13  
**Source:** OpenEvolve Night Cycle 0216

## Problem

With extensive barrel bypass and direct-import performance work, there's no automated way to track startup time improvements or catch regressions.

## Proposal

Add a startup time benchmark to CI that measures gateway initialization time:

```typescript
// benchmark/startup.ts
const start = performance.now();
await import('../src/gateway');
const elapsed = performance.now() - start;
console.log(`startup: ${elapsed.toFixed(1)}ms`);

// Assert no regression beyond threshold
const THRESHOLD_MS = 5000; // 5 seconds
if (elapsed > THRESHOLD_MS) {
  console.error(`Startup regression: ${elapsed.toFixed(1)}ms > ${THRESHOLD_MS}ms`);
  process.exit(1);
}
```

## Benefits

- Quantifiable evidence of barrel bypass campaign impact
- Early detection of startup regressions
- Historical tracking of performance improvements
- Correlation with commit types (perf, feat, fix)

## Implementation Notes

- Should track cold start vs warm start separately
- Consider memory usage alongside timing (process.memoryUsage())
- Results should be stored in CI artifacts for trend analysis
- Threshold should be adjustable per environment

## Related References

- `barrel_bypass_campaign_tracker.md` — Active barrel elimination tracker
- `barrel_import_lint_rule.md` — AST-based ESLint rule for barrel detection
- `perf_budget_ci.md` — Perf budget with import depth tracking

## Cross-References

- OpenClaw commits: 92+ barrel bypass commits by Vincent Koc
- Current estimated completion: ~60% of ~200 barrel sites