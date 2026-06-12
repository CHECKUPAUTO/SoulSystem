# Service Lifecycle Pattern

**Priority:** P1 (from 0318 report)  
**Status:** Proposal  
**Created:** 2026-04-13  
**Source:** night_cycle_20260413_0318.md, night_cycle_20260413_0330.md, night_cycle_20260413_0600.md, night_cycle_20260413_0605.md  

---

## Problem

Gateway startup had a race condition: `startGatewayRuntimeServices()` fired cron + heartbeat BEFORE sidecars completed initialization. This caused `UNAVAILABLE` errors on `chat.history` calls during the early startup window.

**Root cause:** Services initialized in wrong order → runtime errors on first ticks.

## Fix (Upstream #65322)

Two-phase startup pattern:
1. **Phase 1:** Install noop stubs early (safe to call, return nothing)
2. **Phase 2:** Activate real services after dependencies are ready via `activateGatewayScheduledServices()`

This is a **startup sequencing pattern** validated by the fix in commit `92776b8d77`.

## Additional Observations (2026-04-13 06:00)

Three separate commits address startup ordering (#65322, #65365, plus gate startup history), confirming the initialization DAG is complex and growing. The `/health/ready` endpoint recommendation (return 503 until all sidecars report ready) would formalize the external contract.

The formal `GatewayPhase` enum proposal from the 06:05 report:
```typescript
enum GatewayPhase {
  BOOTING = 'booting',
  SIDEWAYS_READY = 'sideways_ready', 
  SERVICES_READY = 'services_ready',
  ACTIVE = 'active',
  SHUTTING_DOWN = 'shutting_down'
}

const PHASE_TRANSITIONS: Record<GatewayPhase, GatewayPhase[]> = {
  [GatewayPhase.BOOTING]: [GatewayPhase.SIDEWAYS_READY],
  [GatewayPhase.SIDEWAYS_READY]: [GatewayPhase.SERVICES_READY],
  [GatewayPhase.SERVICES_READY]: [GatewayPhase.ACTIVE],
  [GatewayPhase.ACTIVE]: [GatewayPhase.SHUTTING_DOWN],
};
```

This makes startup ordering self-documenting and enforces valid transitions at compile time.

## Proposed Pattern: ServiceLifecycle Interface

```typescript
// services/gateway-lifecycle.ts
interface ServicePhase {
  readonly id: string;
  readonly dependsOn: readonly string[];
  activate(): Promise<void>;
}

class ServiceLifecycleManager {
  private phases = new Map<string, ServicePhase>();
  private activated = new Set<string>();

  register(phase: ServicePhase): void {
    this.phases.set(phase.id, phase);
  }

  async activateAll(): Promise<void> {
    // Topological sort by dependsOn, then activate in order
    const order = this.topologicalSort();
    for (const id of order) {
      await this.phases.get(id)!.activate();
      this.activated.add(id);
    }
  }
}
```

## Benefits

- **Explicit dependency declaration** — Services declare what they need before activation
- **No implicit ordering** — The lifecycle manager enforces correct sequencing
- **Testable** — Service phases can be tested in isolation
- **Observable** — Clear log output showing activation order and failures

## Related References

- `startup_context_extraction_pattern.md` — Session state preloading patterns
- `startup_context_performance_monitoring.md` — Performance monitoring for startup
- `circuit_breaker_pattern.md` — Resilience for runtime service failures
- Upstream fix: `92776b8d77` (#65322), `4fec8073b1` (#65365)

## Status Tracking

- [ ] Upstream: `activateGatewayScheduledServices()` merged in #65322
- [ ] Proposal: Generalize to `ServiceLifecycle` interface
- [ ] Proposal: Add dependency declaration to all gateway services