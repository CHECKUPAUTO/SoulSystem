# VideoGenerationAssetRole: Discriminated Union Proposal

**Created:** 2026-04-13 (Night Cycle 00:34)
**Source:** OpenEvolve Night Cycle Report 2026-04-13 00:34
**Status:** Proposal
**Priority:** P1

## Context

The video generation module introduced `VideoGenerationAssetRole` with canonical roles (first_frame, last_frame, reference_image, reference_video, reference_audio) and `validateProviderOptionsAgainstDeclaration()` for runtime schema validation. The current system validates at runtime only.

## Current Design

```typescript
// Runtime-only validation
type VideoGenerationAssetRole = string; // canonical roles listed in docs
const CANONICAL_ROLES = ['first_frame', 'last_frame', 'reference_image', 'reference_video', 'reference_audio'];
// Validation happens at runtime via validateProviderOptionsAgainstDeclaration()
```

## Proposed: Discriminated Union

```typescript
type VideoGenerationAssetRole =
  | { role: 'first_frame'; mediaType: 'image'; required: false }
  | { role: 'last_frame'; mediaType: 'image'; required: false }
  | { role: 'reference_image'; mediaType: 'image'; required: false }
  | { role: 'reference_video'; mediaType: 'video'; required: false }
  | { role: 'reference_audio'; mediaType: 'audio'; required: false };

// Exhaustiveness checking at compile time
function handleAsset(asset: VideoGenerationAssetRole): void {
  switch (asset.role) {
    case 'first_frame': // TypeScript ensures all cases handled
    case 'last_frame':
    case 'reference_image':
    case 'reference_video':
    case 'reference_audio':
      return processAsset(asset);
  }
}
```

### Benefits

1. **Compile-time exhaustiveness** — Missing a role is a build error, not a runtime miss
2. **Media type constraints** — `reference_audio` can't accidentally be used where an image is expected
3. **Extensibility** — Adding a new role requires updating the union (all switch sites flagged)
4. **Documentation** — The type IS the documentation

### Schema Extension: Arrays & Nested Types

```typescript
// Extend validateProviderOptionsAgainstDeclaration for complex providers
type ProviderOptionSchema =
  | { type: 'string' }
  | { type: 'number' }
  | { type: 'boolean' }
  | { type: 'string[]' }        // NEW: multi-select
  | { type: 'record' };          // NEW: nested config

const runwaySchema: Record<string, ProviderOptionSchema> = {
  motion_strength: { type: 'number' },
  reference_images: { type: 'string[]' },  // multiple images
  generation_config: { type: 'record' },     // nested params
};
```

## References

- Video generation role pattern: `evolution/references/video_generation_role_pattern.md`
- Video provider abstraction: `evolution/references/video_provider_abstraction_pattern.md`
- Commits: `2c57ec7b5f`, `f2a4a5ac21`, `b56cd114e7`