# Audio Provider Environment Detection Pattern

**Priority:** P2 (UX Improvement)
**Source:** Night Cycle 2026-04-13 01:47 (commit `94ef2f1`, #65491)
**Status:** Reference documentation
**Created:** 2026-04-13

## Pattern

CLI now auto-detects environment-backed audio providers, reducing configuration burden:

```typescript
// Before: Explicit config required for each audio provider
// After: Environment variables (OPENAI_API_KEY, etc.) auto-configure audio providers
```

## Key Principle

Environment-backed providers should be detected and configured automatically when their required environment variables are present. This reduces the "configuration surface" users need to manage.

## Recommendations

1. **Add E2E test** for detection:
   ```typescript
   describe('CLI Audio Provider Detection', () => {
     it('should detect env-backed providers without explicit config', async () => {});
     it('should fallback gracefully when env vars missing', async () => {});
   });
   ```

2. **Apply pattern to other provider types** — TTS, image generation, video generation could all benefit from env-backed auto-detection

3. **Document the detection contract** — What env vars map to which providers

## Cross-References

- `provider_capability_matrix.md` — Provider capabilities overview
- `model_resolution_cascade.md` — Model resolution patterns