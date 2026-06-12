# Cross-Organ Resonance Protocol

**Source:** OpenEvolve Night Cycle 131 (2026-04-14 14:02)  
**Priority:** HIGH — débloque l'émergence  
**Status:** Proposal (requires manual implementation)

## Current State

Organes communiquent via HTTP REST (synchronous request-response). This limits:
- Simultaneous multi-organ activation
- Attractor synchronization across organs
- Real-time state propagation

## Proposal

Add a pub/sub channel based on UDP multicast local for inter-organ communication:

### Architecture

```
Organ A (publisher)
    │
    ├── UDP multicast → Organ B (subscriber)
    ├── UDP multicast → Organ C (subscriber)
    └── UDP multicast → Organ D (subscriber)
```

### Benefits

1. **Simultaneous Activation:** Multiple organs can receive the same stimulus simultaneously
2. **Attractor Synchronization:** Organ attractors can phase-lock for coherent cognitive states
3. **Lower Latency:** UDP multicast skips TCP handshake overhead for local mesh
4. **Emergent Behavior:** Enables resonance patterns between organs that HTTP cannot support

### Interface Specification

```rust
// Proposed resonance message format
pub struct ResonanceMessage {
    pub source_organ: OrganId,
    pub target_pattern: AttractorPattern,
    pub stimulus: StimulusPayload,
    pub timestamp: u64,
    pub propagation_id: Uuid,
}
```

### Implementation Considerations

- UDP port range: 9050-9060 (reserved for resonance)
- Message size limit: 64KB per datagram
- Sequence numbers for ordering
- Optional reliability via redundant transmission
- IGMP group membership for multicast group management

## Related Proposals

- Hierarchical Mesh (clustering par groupe)
- Dynamic Neuron Allocation (via Homeostasis organ)
- Attractor Transition Logging (observabilité)

## Emergence Impact

This is the single highest-impact architectural change for the mesh. Without it, organs operate in isolation. With it, coherent cognitive states emerge from synchronized attractor dynamics.