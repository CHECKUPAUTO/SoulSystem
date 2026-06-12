# Video Generation Skill Module

**Priority:** P3 (LOW)
**Source:** Night Cycle 2026-04-13 01:15
**Status:** Proposal

## Problem

The video generation API has expanded significantly (providerOptions, inputAudios, imageRoles) but there's no skill-level wrapper for agents to easily use multi-modal generation from context.

## Proposal

Create an OpenClaw skill that wraps the expanded video-gen API into a reusable agent skill.

### Skill Interface

```
/video-gen prompt="sunset over mountains" \
  first-frame=image_url \
  reference-video=video_url \
  provider=google \
  options='{"aspectRatio":"16:9"}'
```

### Features

- **Asset role mapping:** Automatically assign semantic roles (first_frame, reference_image, etc.)
- **Provider capability negotiation:** Check provider capabilities before submitting
- **Circuit breaker:** Built-in retry with exponential backoff (see `video_gen_retry_circuit_breaker.md`)
- **Progress tracking:** Poll generation status and notify on completion

### Skill Structure

```
skills/video-gen/
  SKILL.md          # Usage guide
  scripts/
    generate.sh     # CLI wrapper for video generation API
    status.sh       # Check generation status
    providers.sh    # List available providers and capabilities
```

## Benefits

- Easier multi-modal generation from agent context
- Consistent error handling across providers
- Circuit breaker resilience built-in
- Provider capability awareness prevents #64723-class bugs

## Related References

- `video_generation_role_pattern.md` — Role-based asset pattern
- `video_gen_retry_circuit_breaker.md` — Retry and circuit breaker pattern
- `provider_capability_matrix.md` — Declarative provider capabilities
- `video_provider_abstraction_pattern.md` — Provider adapter interface