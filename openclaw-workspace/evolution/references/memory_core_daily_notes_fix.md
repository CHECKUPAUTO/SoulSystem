# Memory-Core Daily Notes Subdirectory Fix

**Priority:** P0 (from 0332 report — directly affects our workflow)  
**Status:** Fixed upstream (#64682)  
**Created:** 2026-04-13  
**Source:** night_cycle_20260413_0332.md  

---

## Problem (Now Fixed)

Daily notes in `memory/` subdirectories weren't being matched by memory-core. This directly affects our `memory/YYYY-MM-DD.md` workflow — subdirectories were invisible to the daily notes scanner.

## Upstream Fix

Commit (referenced in #64682): Fix memory-core daily notes subdirectory matching.
- +140 lines of tests added
- Properly handles nested directory patterns like `memory/YYYY-MM-DD.md`

## Impact on Our Workspace

Our workspace uses `memory/YYYY-MM-DD.md` for daily notes. This fix means:
- ✅ Daily notes in `memory/` subdirectories are now properly discovered
- ✅ 140 lines of regression tests prevent re-breakage
- No action needed on our part — upstream fix is merged

## Related References

- `dreaming_ltm_architecture.md` — Long-term memory architecture
- `active_memory_design_patterns.md` — Active memory mode presets