# Incremental Test Selection for CI

**Created:** 2026-04-13 (Night Cycle auto-apply)
**Priority:** P3
**Source Reports:** night_cycle_20260413_0102.md
**Status:** Proposal — requires CI pipeline changes

## Problem

CI scaling to 32vCPU signals growing test execution time impacting developer velocity. Running all tests on every PR is wasteful when most changes only affect a subset of modules.

## Proposed Pattern: Affected-Test Selection

Similar to Bazel query (`bazel query`) or Nx affected (`nx affected`), run only tests touching changed modules:

```yaml
# .github/workflows/smart-test.yml
name: Smart Test Selection
on: [pull_request]
jobs:
  affected-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Determine affected modules
        id: modules
        run: |
          CHANGED=$(git diff --name-only origin/main... | \
            grep '^src/' | \
            sed 's|src/\([^/]*\)/.*|\1|' | \
            sort -u | \
            tr '\n' ',')
          echo "modules=$CHANGED" >> $GITHUB_OUTPUT
      - name: Run affected tests
        run: npx vitest run --changed origin/main
```

## Impact

- **40-60% reduction in CI time** on average PRs
- **Faster developer feedback loops**
- **Defers horizontal scaling costs** (32vCPU → stay at current level)
- **Full test suite still runs on merge to main**

## Phased Rollout

1. **Phase 1:** Track affected-test metrics without gating (logging only)
2. **Phase 2:** Gate on affected tests, but allow full-suite fallback
3. **Phase 3:** Default to affected-test selection, full suite on schedule

## Related References

- `evolution/references/perf_budget_ci.md`
- `evolution/references/test_hardening_patterns.md`