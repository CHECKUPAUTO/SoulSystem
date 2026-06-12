# Video Generation Role-Based Asset Pattern

*Created: 2026-04-13 (Night Cycle)*
*Source: night_cycle_20260412_2316.md, night_cycle_20260413_0004.md*

## Overview

Video generation API expansion (`2c57ec7`, #61987) introduces role-based asset pipeline with `providerOptions`, `inputAudios`, and `imageRoles`.

## Pattern: Role-Based Asset Pipeline

```typescript
interface VideoGenerationRequest {
  prompt: string;
  providerOptions?: Record<string, unknown>;  // Opaque passthrough
  inputAudios?: AudioReference[];             // Audio binding
  imageRoles?: ImageRole[];                   // Role-based image assets
}
```

### Design Principles

1. **Provider options are opaque** — `providerOptions` uses `Record<string, unknown>` for forward compatibility. Each provider interprets only what it supports (cf. Google Veo fix `f2a4a5a` — omitting unsupported `numberOfVideos`).

2. **Audio references are bindable** — `inputAudios` enables audio-driven video generation (Seedance 2.0, etc.).

3. **Image roles enable multi-asset composition** — `imageRoles` allows specifying character/background/style references with semantic meaning rather than positional indices.

### Provider Adapter Pattern

```typescript
// Each provider implements a narrow adapter interface
interface VideoProviderAdapter {
  readonly name: string;
  readonly supportedOptions: string[];
  generate(request: NormalizedVideoRequest): Promise<VideoResult>;
}
```

**Risk:** With 14+ video providers, a centralized registry can become a barrel. Apply narrow-surface pattern: each adapter self-registers via `registerVideoProvider()` at startup, no barrel needed.

## Cross-References
- `narrow_surface_pattern.md` — Avoiding barrel patterns
- `build_time_capability_generation.md` — Static capability resolution