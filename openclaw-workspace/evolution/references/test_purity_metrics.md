# Test Purity Metrics Tracking

**Created:** 2026-04-13 (Night Cycle auto-apply)
**Priority:** P2
**Source Reports:** night_cycle_20260413_0048.md, night_cycle_20260413_0053.md
**Status:** Proposal — requires CI pipeline integration

## Context

OpenClaw is in the middle of a systematic test purity migration (14+ commits in the latest batch alone, moving integration tests to pure/unit coverage). This effort is currently invisible — no metrics track progress.

## Proposal: Test Purity Percentage Metric

Add a CI metric that tracks the ratio of pure tests to total tests:

```
purity_ratio = pure_test_files / total_test_files
```

**Target:** 80% pure by Q3 2026

### Implementation Sketch

```bash
#!/bin/bash
# test-purity-metric.sh
total=$(find src/ -name "*.test.ts" | wc -l)
pure=$(find src/ -path "*__pure__*" -name "*.test.ts" | wc -l)
moved=$(git log --oneline --grep="move.*pure" --since="3 months ago" | wc -l)
echo "Total test files: $total"
echo "Pure test files: $pure"
echo "Purity ratio: $(echo "scale=2; $pure/$total" | bc)"
echo "Recently migrated: $moved"
```

### CI Integration

```yaml
# .github/workflows/test-purity.yml
name: Test Purity Check
on: [pull_request]
jobs:
  purity:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Check test purity ratio
        run: |
          total=$(find src/ -name "*.test.ts" | wc -l)
          pure=$(find src/ -path "*__pure__*" -name "*.test.ts" | wc -l)
          ratio=$(echo "scale=2; $pure/$total" | bc)
          echo "Test purity ratio: $ratio (target: >= 0.80)"
```

## Pure Test Migration Scanner

Companion script to identify integration tests eligible for pure-coverage migration:

```bash
#!/bin/bash
# scan-migratable-tests.sh
# Find .test.ts files that have no I/O (no fs, http, database imports)
for f in $(find src/ -name "*.test.ts" -not -path "*__pure__*"); do
  if ! grep -qE "(fs|http|fetch|supertest|mock-fs|redis|sqlite)" "$f"; then
    echo "MIGRATABLE: $f"
  fi
done
```

## Related References

- `evolution/references/pure_test_migration_tracker.md`
- `evolution/references/pure_test_coverage_map.md`
- `evolution/references/pure_test_template.md`