# Video Generation Module Boundaries

**Priority:** Medium (from 0230 report, S2)  
**Status:** Proposal  
**Created:** 2026-04-13  
**Source:** OpenEvolve Night Cycle 0230

## Problem

Video generation added +566 lines (test + runtime) across `providerOptions`, `inputAudios`, `imageRoles`. Without clear module boundaries, the feature risks over-modularization or monolithic growth.

## Proposal: VideoGenerationProvider Interface

```typescript
// media-generation/types.ts
interface VideoGenerationProvider {
  name: string;
  supportedRoles: VideoGenerationAssetRole[];
  maxDuration: number;
  validateOptions(options: Record<string, unknown>): ValidationResult;
  generate(prompt: string, assets: VideoAsset[], options: ProviderOptions): Promise<VideoResult>;
}
```

## Module Boundary Rules

1. **Normalization** — Input validation and normalization stays in `media-generation/runtime-shared.ts`
2. **Provider-specific logic** — Each provider implements the interface independently
3. **Discovery** — Provider registry uses lazy loading; no barrel imports
4. **Testing** — Pure test coverage for normalization; integration tests only for actual provider calls

## Current Status

- `2c57ec7b5f` added `providerOptions`, `inputAudios`, `imageRoles`
- +384 lines test, +182 lines runtime
- 14 providers in the registry

## Related References

- `video_generation_role_pattern.md` — Role-based asset handling
- `video_gen_retry_circuit_breaker.md` — Circuit breaker for video generation
- `video_generation_error_classification.md` — Error classification
- `provider_capability_matrix.md` — Declarative provider capabilities