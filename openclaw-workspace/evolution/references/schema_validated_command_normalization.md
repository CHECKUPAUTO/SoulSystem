# Schema-Validated Command Normalization

**Date:** 2026-04-13  
**Priority:** P2  
**Origin:** night_cycle_20260413_0019

## Context

`commands-registry-normalize.ts` (182 lines) was extracted from the monolithic `commands-registry.ts` as part of the barrel-bypassing campaign. It currently uses raw string parsing with manual validation.

## Proposal

Apply the same `AssertAssignable` pattern used in video generation to command normalization:

```typescript
// commands-registry-normalize.ts
import { z } from "zod";

const CommandInputSchema = z.object({
  raw: z.string(),
  prefix: z.enum(["!", "/", "#"]),
  // ... typed fields
});

export type ValidatedCommand = z.infer<typeof CommandInputSchema>;
```

## Benefits

- Compile-time guarantees on command structure
- Auto-generated types for downstream consumers
- Consistent validation approach across the codebase (matches video generation pattern)
- Better error messages for malformed commands

## Related

- `explicit_seams_pattern.md` — module boundary patterns
- `provider_schema_consistency_ci.md` — CI enforcement of schema consistency
- `barrel_bypassing_guide.md` — origin of the normalization extraction