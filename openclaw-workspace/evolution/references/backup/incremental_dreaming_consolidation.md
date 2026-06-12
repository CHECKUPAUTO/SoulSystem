# Incremental Dreaming Consolidation

**Created:** 2026-04-13 (Night Cycle auto-apply)
**Priority:** P1
**Source Reports:** night_cycle_20260413_0100.md
**Status:** Proposal — requires gateway implementation

## Problem

Current dreaming system performs a full scan of the lookback window every consolidation cycle. For agents with stable memory, this means re-processing unchanged memories every cycle — wasting compute and LLM tokens.

## Proposed Pattern: Delta-Based Consolidation

Track `lastConsolidatedTimestamp` per memory source, only process new entries since last consolidation:

```typescript
interface ConsolidationState {
  source: MemorySource;
  lastConsolidatedTimestamp: number; // epoch ms
  lastConsolidatedId?: string;       // cursor for paginated sources
}

interface DreamingConfig {
  // ... existing config
  incrementalMode: boolean; // default: true
}
```

### Implementation Sketch

```typescript
async function consolidateIncremental(state: ConsolidationState): Promise<ConsolidationResult> {
  const since = state.lastConsolidatedTimestamp;
  const newMemories = await memoryStore.query({
    source: state.source,
    since,
    limit: 100,
  });
  
  if (newMemories.length === 0) {
    return { skipped: true, reason: 'no_new_memories' };
  }
  
  const result = await consolidate(newMemories);
  
  // Update cursor
  state.lastConsolidatedTimestamp = Math.max(...newMemories.map(m => m.timestamp));
  await stateStore.save(state);
  
  return result;
}
```

## Impact

- **~80% reduction in Light dreaming cost** for agents with stable memory
- **Faster consolidation cycles** — only process deltas
- **Lower LLM token usage** — smaller context windows
- **Still allows full scan** — set `incrementalMode: false` for deep/detailed consolidation

## Related References

- `evolution/references/dreaming_ltm_architecture.md`
- `evolution/references/dream_quality_metric.md`
- OpenClaw Dreaming System: Light → Deep → REM phases