# Veo Request Normalization Pattern

**Source:** Night Cycle 2026-04-13 03:30
**Priority:** P2
**Status:** Proposed

## Context
Fix #64723 revealed that `numberOfVideos` was being sent to Google Veo despite being unsupported. This is part of a broader pattern: provider-specific field filtering scattered across runtime modules.

## Recommendation
Expand the existing `normalization.ts` (already +7 lines this cycle) into a shared provider field filter:

```typescript
// Pattern: provider-specific field omissions registry
const PROVIDER_FIELD_OMISSIONS: Record<string, string[]> = {
  google: ['numberOfVideos', ...],
  openai: [...],
  // etc.
};
```

This centralizes the knowledge and reduces future bug surface for new provider integrations.

## Related
- `src/video-generation/normalization.ts`
- `src/video-generation/runtime.ts` (+182 lines this cycle)