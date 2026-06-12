# Codex Harness Circuit Breaker Pattern

**Created:** 2026-04-13 (Night Cycle 00:30)
**Last Updated:** 2026-04-13 01:11
**Source:** OpenEvolve Night Cycle Reports 2026-04-13 00:30, 01:00

### 2026-04-13 Update (01:00 Report)

- Video generation pipeline should also use circuit breaker pattern (see `evolution/references/video_gen_retry_circuit_breaker.md`)
- The `validateProviderOptionsAgainstDeclaration()` pattern is production-grade and should be generalized to other capability systems
- IHarnessExtension interface should be documented for community extensions (Claude Code, Gemini CLI, ACP harnesses)
**Status:** Proposal
**Priority:** P1

## Context

The new Codex app-server harness (`dd26e8c44d`, `31a0b7bd42`) introduces a pluggable agent harness registry with `strict-agentic` execution contract. If Codex becomes unresponsive, the harness should circuit-break rather than retry infinitely.

The circuit breaker pattern from VisionClaw (`evolution/references/circuit_breaker_pattern.md`) should be applied to the Codex harness.

## Proposal

### IHarnessExtension Interface

Document the harness interface contract clearly to enable community extensions (Claude Code, Gemini CLI, ACP harnesses).

```typescript
interface HarnessExtension {
  name: string;
  version: string;
  spawn(config: HarnessSpawnConfig): Promise<HarnessHandle>;
  healthCheck(): Promise<HealthStatus>;
}

interface HarnessSpawnConfig {
  workingDirectory: string;
  model?: string;
  timeout?: number;
  strict?: boolean; // strict-agentic mode
}

interface HarnessHealth {
  status: 'healthy' | 'degraded' | 'unhealthy';
  lastResponseMs: number;
  consecutiveFailures: number;
}
```

### Circuit Breaker Integration

```typescript
interface CircuitBreakerConfig {
  failureThreshold: number;     // failures before opening (default: 3)
  resetTimeoutMs: number;       // time before half-open (default: 30000)
  halfOpenMaxRequests: number;   // test requests in half-open (default: 1)
  monitorIntervalMs: number;     // health check interval (default: 10000)
}

class HarnessCircuitBreaker {
  state: 'closed' | 'open' | 'half-open';
  failureCount: number;
  lastFailureTime: number;
  config: CircuitBreakerConfig;
}
```

### Integration Points

- `plugins/entries/codex/` — Apply circuit breaker around Codex spawn/watchdog
- `plugins/runtime/runtime.js` — Registry-level circuit breaker for harness selection
- `src/harness/` — New directory for harness infrastructure

## References

- VisionClaw circuit breaker pattern: `evolution/references/circuit_breaker_pattern.md`
- Codex harness integration: `evolution/references/codex_harness_integration_guide.md`
- Codex commits: `dd26e8c44d`, `31a0b7bd42`, `84098a2267`, `3b13986214`