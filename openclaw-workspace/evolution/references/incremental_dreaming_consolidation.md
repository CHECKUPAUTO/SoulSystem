# Incremental Dreaming Consolidation Pattern

**Date:** 2026-04-13  
**Source:** Night Cycle Report (01:00)  
**Status:** Proposal  
**Priority:** P1  

## Problem

The current Light dreaming phase does a full scan of the lookback window every cycle (default every 6h). For agents with stable memory, this is wasteful — ~80% of the data hasn't changed since the last consolidation.

## Pattern: Delta-Only Consolidation

```typescript
interface ConsolidationState {
  lastConsolidatedTimestamp: Record<string, number>; // source → last processed timestamp
}

function getUnprocessedEntries(source: MemorySource, state: ConsolidationState): Entry[] {
  const lastTs = state.lastConsolidatedTimestamp[source.id] ?? 0;
  return source.entries.filter(e => e.timestamp > lastTs);
}

async function lightDreamingConsolidation(state: ConsolidationState): Promise<ConsolidationState> {
  for (const source of memorySources) {
    const deltas = getUnprocessedEntries(source, state);
    if (deltas.length === 0) continue; // Skip unchanged sources
    
    await consolidate(deltas);
    state.lastConsolidatedTimestamp[source.id] = Math.max(...deltas.map(e => e.timestamp));
  }
  return state;
}
```

## Benefits

- **~80% reduction** in Light dreaming cost for stable-memory agents
- **Faster cycles** — only process what changed
- **Scales better** — O(changed) not O(total)

## Implementation Notes

1. Persist `ConsolidationState` alongside dreaming configuration
2. Reset state on gateway restart (conservative: re-process everything once)
3. Deep dreaming (nightly) can still do full scans — delta optimization is for Light phase only
4. REM dreaming (weekly) already targets extracted patterns, not raw entries

## Related Patterns

- `dreaming_ltm_architecture.md` — Dreaming/LTM cognitive model
- `session_state_management_patterns.md` — State persistence patterns