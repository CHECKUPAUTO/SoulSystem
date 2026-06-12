# Barrel Bypass Campaign Progress Tracker

**Created:** 2026-04-13
**Source:** Night Cycles 2026-04-13 01:15–02:03

## Status

Campaign ~60% complete. 9-10 perf commits in the latest window, eliminating barrel imports from hot paths.

## Completed Barrel Eliminations

| Commit | Target | Pattern |
|--------|--------|---------|
| perf: avoid plugin index for target normalization | plugin registry | Direct import |
| perf: avoid plugin registry in reply threading | plugin registry | Direct import |
| perf: avoid reply payload barrel in followups | reply payload | Direct import |
| perf: avoid signal approval plugin lookup | signal approval | Static lookup |
| perf: import queue settings directly | queue settings | Direct import |
| perf: short-circuit exact reply suppression targets | reply suppression | Static lookup |
| perf: short-circuit static doctor channel capabilities | doctor capabilities | Static lookup |
| perf: split command body normalization | command body | Pure extraction |
| perf: keep queue and group parsing pure | queue/group | Pure extraction |

**Estimated improvement:** 15-25% reduction in module resolution overhead from these commits alone.

## Remaining Targets

Per 02:00 and 02:03 reports:
- `src/gateway/server-methods/chat.ts` — still has ~30 barrel import lines, 500+ lines total
- Gateway entry point and plugin registry — still using barrel re-exports
- Cold-path barrels — lower priority but worth tracking

## Recommendations

1. **Shift to tree-shaking validation** — Verify production bundle impact. Run `du -sh dist/` before/after. If gains < 5%, declare victory.
2. **Add CI lint rule** — Flag new barrel re-exports in `src/` hot paths to prevent regression
3. **Chat.ts decomposition** — Break out message normalization, session resolution, abort handling

## Cross-References

- `barrel_bypassing_guide.md` — Original guide
- `barrel_import_lint_rule.md` — Lint rule proposal
- `plugin_avoidance_pattern_2026-04-11.md` — Plugin import optimization