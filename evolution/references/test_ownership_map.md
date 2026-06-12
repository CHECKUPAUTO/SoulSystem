# Test Ownership Map

**Priority:** I4 (from 0216 report)  
**Status:** Proposal  
**Created:** 2026-04-13  
**Source:** OpenEvolve Night Cycle 0216

## Problem

The test consolidation campaign (19+ "move to pure" commits) scatters test migrations across many PRs. Without a map, it's hard to track which tests belong to which modules and identify orphaned or misplaced tests.

## Proposal

Create `TEST_OWNERSHIP.md` mapping test files to their "owner" modules:

```markdown
## Test Ownership Map

| Test Directory | Owner Module | Migration Status |
|---|---|---|
| test/pure/plugin-list-formatting.ts | src/plugins/list.ts | ✅ Migrated |
| test/pure/node-pairing-authz.ts | src/nodes/pairing.ts | ✅ Migrated |
| test/pure/sessions-timeout.ts | src/sessions/timeout.ts | ✅ Migrated |
| test/integration/plugin-uninstall.ts | src/plugins/uninstall.ts | 🔄 In Progress |
| test/integration/directive-status.ts | src/directives/status.ts | ⏳ Pending |
```

## Benefits

- Prevents test drift during consolidation campaign
- Makes review of "move to pure" PRs easier
- Identifies orphaned tests (no clear owner)
- Tracks migration progress visually

## Migration Target

- **Goal:** 60% pure tests by v2026.5
- **Current estimate:** ~35% pure, ~65% integration
- **Progress:** 19+ commits migrating tests to pure coverage

## Related References

- `pure_test_coverage_map.md` — Detailed coverage tracking
- `pure_test_migration_tracker.md` — Migration commit history
- `test_purity_metrics.md` — CI metric proposal
- `simplification_wave_tracker.md` — Broader simplification campaign