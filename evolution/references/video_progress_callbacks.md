# Video Generation Progress Callbacks

**Priority:** Medium (from 0219 report, Improvement 2)  
**Status:** Proposal  
**Created:** 2026-04-13  
**Source:** OpenEvolve Night Cycle 0219

## Problem

With audio-to-video support added (`2c57ec7b5f`), video generation times will increase significantly. Currently there's no way for users to track generation progress.

## Proposal

```typescript
// video-progress.ts
interface VideoGenerationProgress {
  stage: 'queued' | 'processing' | 'rendering' | 'complete';
  percent: number; // 0-100
  estimatedTimeRemaining?: number; // seconds
}

// Usage in generation pipeline
const onProgress = (progress: VideoGenerationProgress) => {
  emitToClient('video:progress', progress);
};
```

## Benefits

- Enables UI progress indicators for long-running generations
- Better UX for audio-to-video and multi-asset generations
- Consistent progress reporting across providers (Google, BytePlus, fal)

## Related References

- `video_generation_role_pattern.md` — Role-based asset handling
- `video_gen_retry_circuit_breaker.md` — Retry and circuit breaker for video gen
- `video_generation_error_classification.md` — Error classification across providers
- `video_gen_module_boundaries.md` — Module boundary definition