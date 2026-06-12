# Video Generation Provider Abstraction Pattern

**Created:** 2026-04-12
**Source:** Night cycle 2026-04-12 23:16, commit 2c57ec7b (video_generate: add providerOptions, inputAudios, imageRoles #61987)
**Priority:** Medium — feature still evolving

## Problem

As video generation providers multiply (Veo, Runway, Kling, etc.), `runtime.ts` risks becoming a switch-case dispatcher with provider-specific option bags (`providerOptions`, `inputAudios`, `imageRoles`).

## Pattern: VideoProviderAdapter Interface

```typescript
interface VideoProviderAdapter {
  normalizeOptions(opts: VideoGenerateInput): ProviderSpecificOptions;
  validateOutput(result: ProviderResult): VideoGenerationResult;
}
```

## Current State

- `video-generation/types.ts`: +47 lines for new types (good boundary discipline)
- `runtime.ts`: +182 lines with provider-specific option handling
- Test coverage: +384 lines in `runtime.test.ts`, +355 in `video-generate-tool.test.ts`

## Recommendation

1. Extract `VideoProviderAdapter` interface before adding more providers
2. Each provider implements `normalizeOptions()` and `validateOutput()`
3. Runtime dispatches to adapter, not switch-case
4. Keeps runtime lean as provider count grows

## References

- `src/video-generation/runtime.ts`
- `src/video-generation/types.ts`
- Google Veo fix (f2a4a5a): omit `numberOfVideos` when unsupported — validates the adapter approach