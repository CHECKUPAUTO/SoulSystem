# Hierarchical Mesh Architecture

**Source:** OpenEvolve Night Cycle 131 (2026-04-14 14:02)  
**Priority:** MEDIUM — better isolation and scaling  
**Status:** Proposal (requires orchestrator restructure)

## Current State

The mesh is flat: 6 brain nodes + 7 organ services + 1 orchestrator, all equal peers. This means:
- No logical grouping of related nodes
- All inter-node communication goes through orchestrator
- No isolation boundaries (one failing node affects all)
- No scaling by group (can't scale cognitive group independently)

## Proposal

Restructure into hierarchical clusters:

### Architecture

```
Level 0: Orchestrator (global coordination)
├── Cognitive Group
│   ├── Mind (9011) — core cognition
│   ├── Reasoning (9023/9032) — logical inference
│   ├── Language (9018/9033) — NL processing
│   └── Integration (9022/9036) — cross-node synthesis
├── Physical Group
│   ├── Science (9010) — analysis
│   ├── Engineer (9012) — implementation
│   ├── Crypto (9013) — security
│   └── Perception (9017/9031) — input processing
└── Meta Group
    ├── Creative (9014) — generation
    ├── Meta (9015) — self-reflection
    ├── Affect (9019/9034) — emotional weighting
    └── Reflex (9019/9035) — fast reactive layer
```

### Benefits

1. **Isolation**: Cognitive group failure doesn't crash Physical group
2. **Scaling**: Scale Cognitive group (high reasoning load) independently of Meta group
3. **Routing efficiency**: Intra-group communication doesn't need orchestrator
4. **Resource allocation**: Assign more neurons to high-demand groups
5. **Debugging**: Isolate issues to specific groups

### Implementation Path

1. Define group membership in orchestrator config
2. Add intra-group direct communication (bypass orchestrator for group-local messages)
3. Implement group-level health checks
4. Add group-level resource budgets
5. Enable group-level attractor coordination

### Risks

- Increased configuration complexity
- Potential for group boundary disputes (which group does a node belong to?)
- Cross-group communication latency may increase
- Need clear rules for nodes that bridge groups