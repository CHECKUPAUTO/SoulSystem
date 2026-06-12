# Attractor Transition Logging

**Source:** OpenEvolve Night Cycle 131 (2026-04-14 14:02)  
**Priority:** LOW (observabilité)  
**Status:** Proposal (requires orchestrator modification)

## Current State

Attractor state is visible via `/api/mesh/mind` endpoint but transitions are ephemeral — no historical record of when and why attractors change.

## Proposal

Add a journal of attractor transitions per node:

### Schema

```json
{
  "node": "science",
  "timestamp": "2026-04-14T14:02:00+02:00",
  "from_attractor": "DeepBasin",
  "to_attractor": "StrangeAttractor",
  "trigger": "stimulus:complex_reasoning_task",
  "turbulence_delta": "+0.15",
  "duration_in_previous": "4h23m"
}
```

### Implementation

- Append-only log in RocksDB (per node)
- Exposed via `GET /api/mesh/attractor-log?node=&since=`
- Aggregated view in orchestrator dashboard
- Retention: 30 days, then archive to cold storage

### Expected Benefits

- Understand cognitive regime patterns over time
- Identify which stimuli trigger regime changes
- Correlate attractor state with output quality
- Debug "stuck in wrong attractor" issues
- Feed data back into Homeostasis for self-regulation

### Analysis Opportunities

- **Regime frequency**: How often does each node visit each attractor?
- **Transition triggers**: What stimuli cause DeepBasin → StrangeAttractor?
- **Stability metrics**: How long does each regime last? Is the mesh stable or oscillating?
- **Cross-node correlation**: Do transitions cascade across nodes?