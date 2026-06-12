# Provider Capability Matrix Pattern

**Date:** 2026-04-13  
**Source:** Night Cycle Reports (00:45, 00:48, 00:52, 01:00, 01:02, 01:15, 01:17, 01:30, 01:34)  
**Status:** Proposal  
**Priority:** P1  

## Problem

Multi-provider surfaces (video generation, LLM providers, channel plugins) are fragile. Provider-specific bugs like Google Veo's `numberOfVideos` (issue #64723) and Codex OAuth scope loss (issue #64713) occur because runtime code uses ad-hoc `if (provider === 'X')` guards instead of declarative capability declarations.

## Pattern: Declarative Capability Map

```typescript
interface ProviderCapabilities {
  numberOfVideos: boolean;
  inputAudios: boolean;
  imageRoles: string[];
  maxDuration: number;
  providerOptions: Record<string, ProviderOptionSchema>;
}

const PROVIDER_CAPABILITIES: Record<string, ProviderCapabilities> = {
  google: {
    numberOfVideos: false,  // Veo doesn't support this
    inputAudios: true,
    imageRoles: ['first_frame', 'reference_image'],
    maxDuration: 60,
    providerOptions: { ... }
  },
  byteplus: {
    numberOfVideos: true,
    inputAudios: true,
    imageRoles: ['first_frame', 'last_frame', 'reference_video'],
    maxDuration: 120,
    providerOptions: { ... }
  }
};
```

## Implementation Guidelines

1. **Each provider declares supported features** in a static capability map
2. **Runtime filters options automatically** based on capability map before sending requests
3. **No provider-specific conditionals** in shared code paths
4. **New providers** only need to add their capability entry
5. **Capability gaps** are caught at type-check time, not runtime

## Benefits

- Eliminates entire class of provider-specific filter bugs (like #64723)
- Makes adding new providers a data-driven operation
- Self-documenting: capability map IS the documentation
- Type-safe: TypeScript can validate capability coverage

## Cross-References

- `build_time_capability_generation.md` — Pre-computed static capability maps
- `config_metadata_priority.md` — Related load-order declarations pattern
- `explicit_seams_pattern.md` — The seam between provider capability declaration and runtime filtering

## Upstream Tracking

- Google Veo `numberOfVideos` fix: commit `f2a4a5ac`, issue #64723
- Codex OAuth scope preservation: commit `58708e6f`, issue #64713