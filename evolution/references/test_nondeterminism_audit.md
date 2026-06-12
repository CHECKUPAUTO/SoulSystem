# Test Non-Determinism Audit

**Priority:** P1 (from 0332 report)  
**Status:** Proposal  
**Created:** 2026-04-13  
**Source:** night_cycle_20260413_0332.md  

---

## Problem

In 24 hours, 7 separate test-stability commits were merged:
- `4c8337f` — test(agents): stabilize steer restart ordering
- `de1b6ab` — test(memory-core): freeze dreaming session-ingest clocks
- `feb8e1e` — fix(test): remove duplicate trace directive fixtures
- `9dbbee8` — fix(test): align trace directive type stubs
- `bb064d3` — test(parallels): harden Windows npm smoke
- `cfd5f9e` — test(e2e): repair OpenShell prerelease smoke
- Others

**7 test-stability fixes in 24h suggests systemic non-determinism.**

## Common Patterns Observed

1. **Ordering dependency** — Tests depend on execution order (steer restart)
2. **Time dependency** — Tests break when clocks tick during execution (dreaming session)
3. **Fixture collision** — Duplicate fixtures causing type/shape mismatches
4. **Platform-specific** — Windows-specific test failures
5. **Race conditions in E2E** — Prerelease smoke tests with timing sensitivity

## Proposed Audit

### Phase 1: Categorize (1 day)
- Tag all test files with: `@pure`, `@integration`, `@flaky`
- Identify tests using `Date.now()`, `Math.random()`, `setTimeout`, `setInterval`
- Identify tests depending on file ordering or parallel execution

### Phase 2: Stabilize (1-2 days)
- Freeze time in tests: `vi.useFakeTimers()` or `clock.freeze()`
- Ensure unique fixtures: namespace per test module
- Separate pure from integration: `__pure__/` directories

### Phase 3: CI Guard (ongoing)
- Track flaky test rate in CI metrics
- Add `--shard=auto` for parallel pure test execution
- Flag new `@flaky` tests in PR reviews

## Related References

- `ci_reliability_test_purity.md` — Test purity metrics and CI flake tracking
- `pure_test_migration_tracker.md` — Pure test migration status
- `test_purity_metrics.md` — Pure vs integration test ratio tracking
- `pure_test_coverage_map.md` — Module-level test coverage mapping
- `test_ownership_map.md` — Test ownership by module

## Status Tracking

- [ ] Phase 1: Audit test files for non-determinism patterns
- [ ] Phase 2: Implement stabilization fixes
- [ ] Phase 3: Add CI flake tracking