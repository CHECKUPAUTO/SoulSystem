# Deferred Activation Pattern

**Priority:** P1 (High)
**Source:** Night Cycle 2026-04-13 04:31, 04:05 (commits 92776b8d, 4fec8073, 6a796173)
**Status:** Reference documentation
**Applies to:** Gateway startup, sidecar initialization, VisionClaw WebSocket

---

## Problem

Race conditions occur when gateway scheduled services (cron, heartbeat) start before sidecars and runtime services are fully initialized. This causes `UNAVAILABLE` errors for any heartbeat/cron calling `chat.history` during the init window.

## Pattern: Noop Placeholder → Post-Attach Activation → Health Gate

```typescript
// Phase 1: Register noop placeholder during early startup
function registerNoopHeartbeat() {
  return {
    async beat() { /* no-op until activated */ },
    isActive: false
  };
}

// Phase 2: Activate after sidecars ready
function activateRealHeartbeat(runtime: GatewayRuntime) {
  heartbeat.beat = runtime.heartbeat;
  heartbeat.isActive = true;
}

// Phase 3: Health gate - only fire if active
async function onHeartbeatTrigger() {
  if (!heartbeat.isActive) {
    logger.debug('heartbeat skipped: not yet activated');
    return;
  }
  await heartbeat.beat();
}
```

## Implementation Details

From commit `92776b8d` (#65322):
- `startGatewayRuntimeServices()` previously started cron + heartbeat BEFORE sidecars
- Fix: noop heartbeat placeholder early, real activation after `startGatewayPostAttachRuntime()`
- Companion commit `6a796173` defers scheduled services similarly

From commit `4fec8073` (#65365):
- Gates startup history & model requests until services are ready

## Cross-Project Application

### VisionClaw WebSocket v3
VisionClaw's WebSocket init currently uses exponential backoff. Should adopt the same "defer until ready" pattern:
- Register noop WebSocket handlers during early init
- Activate real handlers after device connection is established
- Health gate: only process WebSocket frames when connection is confirmed ready

### Generic Sidecar Pattern
Any system with sidecars should use this pattern:
1. **Early registration:** Register no-op stubs for all sidecar-dependent services
2. **Post-attach activation:** Replace stubs with real implementations after sidecars initialize
3. **Health gate:** All scheduled/periodic tasks check readiness before executing

## Anti-Patterns to Avoid

- ❌ Starting cron before database is ready
- ❌ Firing heartbeats before chat history is available
- ❌ Using exponential backoff as a substitute for readiness gates (masks underlying timing issues)
- ❌ Treating `UNAVAILABLE` errors during startup as transient failures (they're structural)

## Related References

- `startup_context_extraction_pattern.md` — Session state preloading
- `service_lifecycle_pattern.md` — Two-phase startup with dependency declarations
- `watchdog_cron_decoupling.md` — Independent watchdog timer pattern
- `circuit_breaker_pattern.md` — Runtime failure handling (complementary to deferred activation)

## Key Insight

Deferred activation and circuit breakers are complementary:
- **Deferred activation** prevents init-time race conditions (services fire too early)
- **Circuit breakers** handle runtime failures (services break after working)
- Together they form a complete lifecycle: **start-safe → run-resilient**