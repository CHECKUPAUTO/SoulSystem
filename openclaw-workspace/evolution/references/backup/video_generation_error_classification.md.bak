# Video Generation Error Classification Unification

**Priority:** P2 (MEDIUM)
**Source:** Night Cycle 2026-04-13 01:17
**Status:** Proposal

## Problem

The video generation runtime handles provider errors with ad-hoc mapping. Different providers (Google Veo, BytePlus, fal) each have their own error handling logic, leading to:
- Duplicate error mapping in `byteplus/video-generation-provider.ts` and `google/video-generation-provider.ts`
- No standardized retry/circuit-breaker behavior
- Inconsistent error reporting to users

## Proposal

Create a shared `VideoGenerationError` classification layer, analogous to how Feishu uses `comment-shared.ts` for shared utilities.

### Architecture

```
types.ts (provider-agnostic)
  ↓
normalization.ts
  ↓
runtime.ts
  ↓
errors/
  video-generation-error.ts    ← New: shared error classification
  provider-error-mappings.ts   ← New: provider-specific error mapping
  ↓
provider.ts
```

### Error Classification

```typescript
class VideoGenerationError extends Error {
  readonly code: VideoGenerationErrorCode;
  readonly provider: string;
  readonly retryable: boolean;
  readonly retryAfter?: number;  // seconds
  readonly cause?: Error;
}

enum VideoGenerationErrorCode {
  // Provider errors
  PROVIDER_UNAVAILABLE,      // 5xx, network timeout
  PROVIDER_RATE_LIMITED,     // 429
  PROVIDER_QUOTA_EXCEEDED,   // 402/403
  PROVIDER_INVALID_REQUEST,  // 400
  
  // Content errors
  CONTENT_POLICY_VIOLATION,  // Safety filter
  CONTENT_UNSUPPORTED,        // Unsupported format/feature
  
  // Auth errors
  AUTH_EXPIRED,               // Token refresh needed
  AUTH_INVALID,               // Bad credentials
  
  // System errors
  TIMEOUT,                    // Generation exceeded time limit
  CIRCUIT_OPEN,               // Circuit breaker tripped
}
```

### Provider Error Mapping

```typescript
// Each provider maps its native errors to VideoGenerationError
const googleErrorMap: ProviderErrorMap = {
  429: { code: PROVIDER_RATE_LIMITED, retryable: true, retryAfter: 60 },
  400: { code: PROVIDER_INVALID_REQUEST, retryable: false },
  // ...
};

const byteplusErrorMap: ProviderErrorMap = {
  // BytePlus-specific mappings
};
```

### Benefits

- Eliminates duplicate error mapping logic across providers
- Enables standardized retry/circuit-breaker behavior (see `video_gen_retry_circuit_breaker.md`)
- Consistent error messages to users regardless of provider
- Easy to add new providers — just define the error map

### Related References

- `video_gen_retry_circuit_breaker.md` — Retry and circuit breaker integration
- `provider_capability_matrix.md` — Declarative provider capabilities
- `circuit_breaker_pattern.md` — General circuit breaker pattern
- Feishu `comment-shared.ts` — Reference pattern for shared utility extraction