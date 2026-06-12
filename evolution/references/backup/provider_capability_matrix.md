# Provider Capability Matrix Pattern

**Created:** 2026-04-13 (Night Cycle auto-apply)
**Priority:** P1
**Source Reports:** night_cycle_20260413_0034.md, night_cycle_20260413_0045.md, night_cycle_20260413_0048.md
**Status:** Proposal — requires TypeScript implementation

## Problem

Multi-provider surfaces (video generation, image generation, TTS, STT) have ad-hoc provider-specific guards scattered across call sites:

```typescript
// Current: fragile, scattered
if (provider === 'google') {
  delete options.numberOfVideos; // Veo doesn't support this
}
```

This pattern creates a class of bugs where provider-specific filtering is done at call time rather than declared upfront. Example: #64723 (Google Veo `numberOfVideos` param).

## Proposed Pattern

Replace ad-hoc `if (provider === ...)` guards with a declarative capability map per provider:

```typescript
interface ProviderCapabilities {
  numberOfVideos: boolean;
  inputAudios: boolean;
  imageRoles: readonly VideoGenerationAssetRole[];
  maxDuration: number;
  // ... extensible via provider-specific overrides
}

const PROVIDER_CAPABILITIES: Record<VideoProvider, ProviderCapabilities> = {
  google: {
    numberOfVideos: false,
    inputAudios: true,
    imageRoles: ['first_frame', 'reference_image'],
    maxDuration: 60,
  },
  fal: {
    numberOfVideos: true,
    inputAudios: false,
    imageRoles: ['first_frame', 'last_frame', 'reference_image', 'reference_video'],
    maxDuration: 300,
  },
  // ...
};
```

Runtime then filters options based on declared capabilities:

```typescript
function filterProviderOptions(provider: VideoProvider, options: VideoGenOptions): VideoGenOptions {
  const caps = PROVIDER_CAPABILITIES[provider];
  const filtered = { ...options };
  if (!caps.numberOfVideos) delete filtered.numberOfVideos;
  if (!caps.inputAudios) delete filtered.inputAudios;
  filtered.imageRoles = options.imageRoles?.filter(r => caps.imageRoles.includes(r));
  return filtered;
}
```

## Benefits

1. **Eliminates entire class of provider-specific filter bugs** — all filtering is declarative
2. **Self-documenting** — capability map IS the documentation
3. **Extensible** — new providers just add an entry
4. **Testable** — can unit test capability declarations independently
5. **Generalizable** — same pattern applies to image gen, TTS, STT

## Related Patterns

- `validateProviderOptionsAgainstDeclaration()` from video generation (#61987)
- `AssertAssignable<SchemaA, SchemaB>` pattern for SDK type compatibility
- `evolution/references/provider_options_validation_pattern.md`

## Cross-References

- Issue #64723: Google Veo numberOfVideos fix
- `evolution/references/video_generation_role_pattern.md`
- `evolution/references/video_provider_abstraction_pattern.md`