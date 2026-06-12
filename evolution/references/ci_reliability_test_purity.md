# CI Reliability and Test Purity Tracking

**Date:** 2026-04-13  
**Source:** Night Cycle Reports (00:45, 00:48, 01:15, 01:30)  
**Status:** Proposal  
**Priority:** P2  

## Problem

The OpenClaw codebase has 14+ test consolidation commits (moving integration tests to pure/unit coverage) but no metrics tracking progress. CI is scaling to 32vCPU runners, suggesting growing test execution time. Without visibility, it's unclear whether consolidation efforts are improving things.

## Pattern: Test Purity Metrics

```yaml
# CI pipeline addition
test-purity:
  script: |
    # Count pure vs integration test files
    PURE=$(find src -name "*.spec.ts" -o -name "*.test.ts" | grep -v integration | wc -l)
    INTEGRATION=$(find src -name "*.integration.test.ts" | wc -l)
    TOTAL=$((PURE + INTEGRATION))
    RATIO=$(echo "scale=2; $PURE / $TOTAL" | bc)
    echo "Test purity: ${RATIO} (${PURE}/${TOTAL} pure)"
  artifacts:
    - test-purity-metrics.json
```

## Targets

- **Q2 2026**: 75% pure tests (from estimated ~60%)
- **Q3 2026**: 80% pure tests
- **CI SLA**: >95% green on first run

## CI Flake Tracking

Add annotation to flaky tests:

```typescript
// @flaky-retry 3 — fails intermittently on CI, see #XXXXX
describe('integration: cron regression', () => { ... });
```

Track in CI artifacts:
- Test name
- Retry count
- Failure reason category (timeout, race, network, flaky assertion)

## Related

- `test_mock_consolidation_guide.md` — Existing test consolidation patterns
- CI runner scaling to 32vCPU indicates growing test surface