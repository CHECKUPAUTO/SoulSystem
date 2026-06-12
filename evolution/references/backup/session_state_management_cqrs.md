# Session State Management - CQRS Pattern

**Source:** Night Cycle Analysis (PolymathicAI Hydra config patterns)
**Status:** Architectural recommendation

---

## Problem Statement

Current: `sessions.patch` WS method with manual state tracking
Issue: No centralized state management for cross-session coordination

---

## Recommendation: Event-Sourced Session State with CQRS

### Benefits
- Better debugging with event replay capability
- Distributed consistency for multi-node deployments
- Audit trail for all state changes

### Pattern

```typescript
// Event Store
interface SessionEvent {
  eventId: string;
  sessionId: string;
  type: 'created' | 'updated' | 'closed' | 'tool_called';
  payload: unknown;
  timestamp: Date;
  vectorClock: number;
}

// Command Side (Write)
interface SessionCommand {
  execute(eventStore: EventStore): Promise<void>;
}

// Query Side (Read)
interface SessionQuery {
  execute(readModel: ReadModel): Promise<SessionView>;
}

// Separation of concerns
class SessionCommandHandler {
  async handle(command: SessionCommand): Promise<void> {
    const events = command.execute();
    await this.eventStore.append(events);
    await this.projection.update(events);
  }
}

class SessionQueryHandler {
  async handle(query: SessionQuery): Promise<SessionView> {
    return query.execute(this.readModel);
  }
}
```

---

## Hydra-Style Configuration

Inspired by PolymathicAI's composable configuration:

```yaml
# session-config.yaml
session:
  state_management:
    mode: "event_sourced"
    event_store: "sqlite"  # or redis, postgres
    projection: "in_memory"  # or materialized views
    
  cqrs:
    command_bus: "synchronous"  # or message_queue
    query_cache: "lru"  # with TTL
    
  consistency:
    model: "eventual"  # or strong for critical paths
    replication: "async"  # for distributed setups
```

---

## Implementation Phases

### Phase 1: Event Store Foundation
- Create SessionEvent schema
- Implement append-only event log
- Add event serialization (JSON5/BSON)

### Phase 2: Projections
- Create read models for common queries
- Implement projection handlers
- Add read model caching

### Phase 3: Query API
- Migrate existing queries to read models
- Add query optimization
- Implement query-side caching

### Phase 4: Distributed Support
- Add event bus for cross-node replication
- Implement vector clocks for ordering
- Add conflict resolution

---

## Benefits for OpenClaw

1. **Debugging**: Replay any session from event log
2. **Testing**: Deterministic test setup via event injection
3. **Scaling**: Read replicas for query load
4. **Analytics**: Event stream for usage analysis
5. **Recovery**: Point-in-time recovery from events

---

## References

- PolymathicAI Hydra: https://github.com/PolymathicAI
- Night Cycle: night_cycle_20260411_0716.md
- Related: `session_state_management_patterns.md`
