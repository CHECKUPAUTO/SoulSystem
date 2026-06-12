# Runtime Migration Tracker

**Based on OpenEvolve Night Cycle Report 2026-04-11**  
**Generated:** Auto-applied 2026-04-11 23:08 UTC

## Overview

Tracks the migration progress from mixed state/logic modules to separated `.runtime.ts` and `*-state.ts` patterns.

## Migration Progress

### Completed Modules ✅

| Module | State File | Business Logic | Status |
|--------|-----------|----------------|--------|
| Context | `context-runtime-state.ts` | `context.ts` | ✅ Migrated |
| Models Config | `models-config-state.ts` | `models-config.ts` | ✅ Migrated |
| Store Lock | `store-lock-state.ts` | `store-lock.ts` | ✅ Migrated |
| Media | `media/store.runtime.ts` | `media/store.ts` | ✅ Migrated |

### Pending Migration ⏳

| Module | Estimated Complexity | Priority |
|--------|---------------------|----------|
| Plugin State | Medium | High |
| Session State | High | Medium |
| Agent State | High | Medium |
| Tool State | Medium | Low |

## Migration Checklist Template

```markdown
- [ ] Identify mixed state/logic in module
- [ ] Create `{module}-state.ts` with pure state + selectors
- [ ] Create `{module}.runtime.ts` for runtime-dependent code
- [ ] Update imports in dependent modules
- [ ] Add tests for extracted state module
- [ ] Verify no import cycles introduced
- [ ] Update documentation
```

## Estimation

- **Total Modules:** ~15 state-bearing modules
- **Migrated:** 4 (27%)
- **Remaining:** 11 (73%)
- **Estimated Completion:** 2-3 weeks at current velocity

## Related Patterns

- See `session_state_management_patterns.md` for implementation details
- See `test_mock_consolidation_guide.md` for testing approach

---
*Auto-generated from Night Cycle analysis*
*Last Updated: 2026-04-11*
