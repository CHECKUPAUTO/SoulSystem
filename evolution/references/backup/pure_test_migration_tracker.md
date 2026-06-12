# Pure Test Migration Tracker

**Created:** 2026-04-12
**Source:** Night cycle 2026-04-12 23:05 analysis

## Status

Active migration wave (10 commits as of 2026-04-12).

## Commits Tracking Pure Test Migration

| Commit | Description | Status |
|--------|-------------|--------|
| Moving tests to owner modules | Pure test extraction | ✅ Migrated |
| Plugin list formatting | Pure tests | ✅ Migrated |
| Node pairing authz | Pure coverage | ✅ Migrated |
| Sessions timeout checks | Pure coverage | ✅ Migrated |
| Plugin uninstall/update selection | Pure tests | ✅ Migrated |
| Queue and group parsing | Pure | ✅ Migrated |
| Command body normalization | Pure split | ✅ Migrated |

## Update 2026-04-12 23:05

Night cycle identified 10 pure test migration commits in the April 11 wave (Peter Steinberger).
Additionally: cron regression harness hardened (6883273), and 3 more pure-test commits (plugin list formatting, node pairing authz, sessions timeout).

Total pure test migrations tracked: 10+

### Feishu Module Note
- `monitor.comment.ts` grew +761 lines in ebb72bab — candidate for decomposition
- `comment-dispatcher.ts` extracted but bulk remains in god module
- See: `feishu_modularization_guide.md`

## Update 2026-04-12 23:56

Night cycles 23:45-23:47 identified 12 additional pure test extraction commits:
- Plugin list/uninstall/update → dedicated pure test files
- Node pairing authz → `node-pairing-authz.test.ts`
- Directive handling → split into model, queue-validation, mixed-inline tests
- Command body normalization → `commands-registry-normalize.ts` (explicit seams pattern)

Total pure test migrations tracked: 22+

**Architecture health:** ~65% pure test ratio, trajectory suggests ~60% by next week.
**Pattern:** Each commit targets one concern — consistent with barrel-bypassing and explicit-seams patterns.
**Refactor+perf to feat ratio:** 13:1 — maturity phase focused on debt repayment.

### Key Risks
- Feishu `comment-shared.ts` at 331 lines — growing barrel risk
- Video generation `types.ts` added 47 lines — 14 providers need registry pattern
- Codex OAuth scope handler 287 lines of tests — candidate for scope-manager extraction

## Next Steps

- Generate coverage map of remaining integration-only tests
- Automate pure-test detection (identify tests that don't need I/O)
- Prioritize by execution time savings
- Create test-purity-dashboard metric (pure/integration ratio tracking)
