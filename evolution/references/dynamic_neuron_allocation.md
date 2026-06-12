# Dynamic Neuron Allocation

**Source:** OpenEvolve Night Cycle 131 (2026-04-14 14:02)  
**Priority:** MEDIUM — amélioration d'efficacité  
**Status:** Proposal (requires Homeostasis organ implementation)

## Current State

All 6 brain nodes have a fixed allocation of 400 neurons each (total 2400). This means:
- Idle nodes consume the same neural capacity as active ones
- High-load nodes cannot borrow capacity from dormant neighbors
- No adaptive response to varying workload patterns

## Proposal

Dynamic neuron allocation based on load (200-800 range per node):

### Architecture

```
Allocation Controller (in Homeostasis organ):
├── Monitor load per node (request rate, activation %, pressure ratio)
├── Compute allocation delta: more load → more neurons (up to 800)
├── Reclaim from dormant nodes: idle → minimum 200
├── Smooth transitions: no sudden jumps, ramp over 30-60s
└── Safety floor: every node keeps minimum 200 neurons

Constraints:
- Total budget: 2400 neurons (can expand to 4800 if memory allows)
- No node below 200 neurons (minimum operational capacity)
- No node above 800 neurons (diminishing returns beyond this)
- Reallocation rate limited: max 100 neurons/minute per node
```

### Implementation Dependencies

- **Homeostasis organ** (proposal 2 from cycle 131) must be implemented first
- Requires mesh bridge support for dynamic N parameter
- Monitoring infrastructure to track per-node load

### Expected Benefits

- Active nodes get up to 2x capacity when needed
- Dormant nodes release 50% of neural capacity
- Overall mesh efficiency improvement estimated 30-50%
- Enables load-adaptive behavior

### Risks

- Frequent reallocation could cause instability
- Need cooldown periods after reallocation
- Must preserve attractor stability during neuron count changes