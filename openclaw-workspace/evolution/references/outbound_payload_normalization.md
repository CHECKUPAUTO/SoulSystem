# Outbound Payload Normalization Pattern

**Priority:** P1 (from 0318, 0330 reports)  
**Status:** Proposal  
**Created:** 2026-04-13  
**Source:** night_cycle_20260413_0318.md, night_cycle_20260413_0330.md  

---

## Context

A major refactoring (commit `c4764095f8`, 967 insertions across 25 files) replaced dual normalization paths:

**Before:** `normalizeOutboundPayloads()` + `normalizeOutboundPayloadsForJson()` — two separate normalization passes for different consumers.

**After:** `createOutboundPayloadPlan()` → `projectOutboundPayloadPlanForJson()` / `projectOutboundPayloadPlanForOutbound()` — a single plan that projects to different views.

## Key Insight

The old pattern normalized **twice** (once for JSON, once for outbound), causing:
- Duplicate merge logic
- Inconsistent normalization between the two paths
- Reply directive parsing executed twice

The new pattern creates **one plan**, then projects it to different views. This eliminates redundant parsing and ensures consistency.

## Proposed Pattern: PayloadBuilder<T>

Generalize the plan/project approach:

```typescript
interface PayloadPlan<T> {
  readonly source: T;
  readonly directives: ReplyDirective[];
  readonly metadata: PayloadMetadata;
  projectForJson(): JsonView;
  projectForOutbound(): OutboundView;
}

class PayloadBuilder<T> {
  private directives: ReplyDirective[] = [];
  private metadata: PayloadMetadata = {};

  fromSource(source: T): this { /* ... */ }
  withDirective(directive: ReplyDirective): this { /* ... */ }
  withMetadata(key: string, value: unknown): this { /* ... */ }
  
  build(): PayloadPlan<T> {
    return new ConcretePayloadPlan(this.source, this.directives, this.metadata);
  }
}
```

## Benefits

- **Type-safe construction** — Each channel gets its own builder specialization
- **Single normalization pass** — Directives parsed once, projected as needed
- **Prevents drift** — Builder pattern prevents ad-hoc normalization bypasses
- **Channel-specific views** — Discord, WhatsApp, Telegram each project differently

## Related References

- `narrow_surface_pattern.md` — Reducing API surface area
- `explicit_seams_pattern.md` — Module boundary patterns
- `barrel_bypassing_guide.md` — Eliminating circular dependencies

## Status Tracking

- [x] Upstream: `payloads.ts` merged with plan/project pattern
- [ ] Proposal: Generalize to `PayloadBuilder<T>` for other channels
- [ ] Proposal: Add property-based fuzz testing for multi-channel normalization