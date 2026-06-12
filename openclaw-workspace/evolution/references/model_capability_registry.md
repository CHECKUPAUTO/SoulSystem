# Model Capability Registry Pattern

**Priority:** P1 (from 0332 report)  
**Status:** Proposal  
**Created:** 2026-04-13  
**Source:** night_cycle_20260413_0332.md  

---

## Problem

GPT-5.4 runtime completion rollup (commit `26945ddb`, #65219) introduced `resolveEffectiveExecutionContract()` with **hardcoded GPT-5 model ID detection**. This creates implicit behavior based on model identity strings rather than declared capabilities.

**Risk:** Surprise regressions when new models are added. The hardcoded check:
```typescript
if (modelId.includes('gpt-5')) {
  // auto-activate strict-agentic
}
```

## Proposed Solution: Declarative Capability Map

Replace hardcoded model checks with a **per-model capability registry**:

```typescript
interface ModelCapabilities {
  readonly strictAgentic: boolean;
  readonly maxOutputTokens: number;
  readonly supportsStreaming: boolean;
  readonly supportsVision: boolean;
  readonly supportsToolChoice: boolean;
  // ... extensible
}

const MODEL_CAPABILITIES: Record<string, ModelCapabilities> = {
  'gpt-5': { strictAgentic: true, maxOutputTokens: 32768, ... },
  'gpt-5.4': { strictAgentic: true, maxOutputTokens: 65536, ... },
  'gpt-4.1': { strictAgentic: false, maxOutputTokens: 16384, ... },
  // ... declarative per model
};
```

Then the contract resolution becomes:
```typescript
const capabilities = MODEL_CAPABILITIES[modelId] ?? DEFAULT_CAPABILITIES;
if (capabilities.strictAgentic) {
  // activate strict-agentic contract
}
```

## Benefits

- **Explicit** — Capabilities are declared, not inferred from string matching
- **Extensible** — Adding a new model is a data change, not a code change
- **Testable** — Capability lookups are pure functions
- **Safe** — Default capabilities provide fallback for unknown models

## Related References

- `startup_context_extraction_pattern.md` — Explicit seams for service configuration
- `codex_harness_integration_guide.md` — Agent execution contracts
- `provider_capability_matrix.md` — Provider-level capability declarations

## Status Tracking

- [ ] Upstream: `resolveEffectiveExecutionContract()` merged with hardcoded check
- [ ] Proposal: Refactor to capability map lookup
- [ ] Proposal: Add CI validation that new models have capability entries