# Session State Audit Trail

**Source:** OpenEvolve Night Cycle Report 2026-04-12 03:15  
**Priority:** P2  
**Related Commits:** 4d0f555 (startup context), c31aa6da (active memory context preservation)

---

## Background

The recent session reset and active memory fixes indicate complex state management that would benefit from structured audit trails. This pattern provides a way to track and debug session state transitions.

---

## Audit Trail Design

### 1. Core Types

```typescript
// src/session/audit-trail.ts
export interface SessionStateTransition {
  transitionId: string;
  sessionId: string;
  from: SessionState;
  to: SessionState;
  reason: TransitionReason;
  metadata: TransitionMetadata;
  timestamp: number;
  duration?: number;
}

export type SessionState =
  | 'initializing'
  | 'active'
  | 'paused'
  | 'resetting'
  | 'recalling'
  | 'terminating'
  | 'terminated';

export type TransitionReason =
  | 'startup'
  | 'reset'
  | 'recall'
  | 'manual'
  | 'error'
  | 'timeout'
  | 'user_request';

export interface TransitionMetadata {
  memoryPreloaded?: boolean;
  contextPreserved?: boolean;
  parentChannelId?: string;
  errorType?: string;
  errorMessage?: string;
  memoryEntryCount?: number;
  // Extension-specific
  extensionData?: Record<string, unknown>;
}
```

### 2. Audit Trail Logger

```typescript
// src/session/audit-trail-logger.ts
import { EventEmitter } from 'events';

export class SessionAuditTrail extends EventEmitter {
  private transitions: Map<string, SessionStateTransition[]> = new Map();
  private maxHistoryPerSession = 100;

  recordTransition(transition: Omit<SessionStateTransition, 'transitionId'>): void {
    const fullTransition: SessionStateTransition = {
      ...transition,
      transitionId: this.generateId(),
    };

    // Store in session history
    const history = this.transitions.get(transition.sessionId) || [];
    history.push(fullTransition);

    // Trim old entries
    if (history.length > this.maxHistoryPerSession) {
      history.shift();
    }

    this.transitions.set(transition.sessionId, history);

    // Emit for real-time monitoring
    this.emit('transition', fullTransition);

    // Log based on severity
    this.logTransition(fullTransition);
  }

  getHistory(sessionId: string): SessionStateTransition[] {
    return this.transitions.get(sessionId) || [];
  }

  getCurrentState(sessionId: string): SessionState | undefined {
    const history = this.getHistory(sessionId);
    return history[history.length - 1]?.to;
  }

  private logTransition(transition: SessionStateTransition): void {
    const logData = {
      transitionId: transition.transitionId,
      sessionId: transition.sessionId,
      from: transition.from,
      to: transition.to,
      reason: transition.reason,
      duration: transition.duration,
      ...(transition.metadata.errorType && {
        error: transition.metadata.errorType,
      }),
    };

    if (transition.metadata.errorType) {
      logger.error('Session state transition with error', logData);
    } else if (transition.reason === 'reset' || transition.reason === 'recall') {
      logger.warn('Session state transition', logData);
    } else {
      logger.debug('Session state transition', logData);
    }
  }

  private generateId(): string {
    return `tr-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
  }
}

// Singleton instance
export const auditTrail = new SessionAuditTrail();
```

### 3. Integration Points

```typescript
// src/session/session-manager.ts
import { auditTrail } from './audit-trail-logger';

export class SessionManager {
  async resetSession(sessionId: string, options?: ResetOptions): Promise<void> {
    const startTime = performance.now();
    const previousState = auditTrail.getCurrentState(sessionId) || 'active';

    // Record transition start
    auditTrail.recordTransition({
      sessionId,
      from: previousState,
      to: 'resetting',
      reason: 'reset',
      metadata: {
        memoryPreloaded: false,
      },
      timestamp: Date.now(),
    });

    try {
      // Perform reset
      await this.performReset(sessionId, options);

      // Record success
      auditTrail.recordTransition({
        sessionId,
        from: 'resetting',
        to: 'active',
        reason: 'reset',
        metadata: {
          memoryPreloaded: options?.preloadMemory ?? false,
          memoryEntryCount: options?.memoryEntries?.length,
        },
        timestamp: Date.now(),
        duration: performance.now() - startTime,
      });
    } catch (error) {
      // Record failure
      auditTrail.recordTransition({
        sessionId,
        from: 'resetting',
        to: previousState, // Rollback
        reason: 'reset',
        metadata: {
          memoryPreloaded: false,
          errorType: error.name,
          errorMessage: error.message,
        },
        timestamp: Date.now(),
        duration: performance.now() - startTime,
      });
      throw error;
    }
  }

  async recallSession(sessionId: string, recallOptions: RecallOptions): Promise<Session> {
    const startTime = performance.now();

    auditTrail.recordTransition({
      sessionId,
      from: 'active',
      to: 'recalling',
      reason: 'recall',
      metadata: {
        parentChannelId: recallOptions.parentChannelId,
        contextPreserved: false,
      },
      timestamp: Date.now(),
    });

    try {
      const session = await this.performRecall(sessionId, recallOptions);

      auditTrail.recordTransition({
        sessionId,
        from: 'recalling',
        to: 'active',
        reason: 'recall',
        metadata: {
          parentChannelId: recallOptions.parentChannelId,
          contextPreserved: !!session.parentContext,
        },
        timestamp: Date.now(),
        duration: performance.now() - startTime,
      });

      return session;
    } catch (error) {
      auditTrail.recordTransition({
        sessionId,
        from: 'recalling',
        to: 'active',
        reason: 'recall',
        metadata: {
          errorType: error.name,
          errorMessage: error.message,
        },
        timestamp: Date.now(),
        duration: performance.now() - startTime,
      });
      throw error;
    }
  }
}
```

### 4. Active Memory Extension Integration

```typescript
// extensions/active-memory/index.ts
import { auditTrail } from '../../src/session/audit-trail-logger';

export class ActiveMemoryExtension {
  async recall(options: RecallOptions): Promise<Session> {
    const session = await this.internalRecall(options);

    // Record context preservation success/failure
    auditTrail.recordTransition({
      sessionId: session.id,
      from: 'active',
      to: 'active',
      reason: 'recall',
      metadata: {
        parentChannelId: options.channelId,
        contextPreserved: this.isContextPreserved(session, options),
        memoryEntryCount: session.memory?.length,
        extensionData: {
          activeMemoryVersion: this.version,
          recallStrategy: options.strategy,
        },
      },
      timestamp: Date.now(),
    });

    return session;
  }

  private isContextPreserved(session: Session, options: RecallOptions): boolean {
    return !!session.channelContext?.parentChannelId &&
           session.channelContext.parentChannelId === options.parentChannelId;
  }
}
```

---

## Debugging with Audit Trail

### 1. Session State Inspector

```typescript
// src/debug/session-inspector.ts
export class SessionInspector {
  constructor(private auditTrail: SessionAuditTrail) {}

  printSessionHistory(sessionId: string): void {
    const history = this.auditTrail.getHistory(sessionId);

    console.log(`\n📋 Session History: ${sessionId}`);
    console.log('=' .repeat(60));

    for (const transition of history) {
      const time = new Date(transition.timestamp).toISOString();
      const duration = transition.duration ? `(${transition.duration.toFixed(2)}ms)` : '';
      const icon = this.getStateIcon(transition.to);

      console.log(`${icon} ${time} | ${transition.from} → ${transition.to} [${transition.reason}] ${duration}`);

      if (transition.metadata.errorType) {
        console.log(`   ⚠️  Error: ${transition.metadata.errorType}`);
      }

      if (transition.metadata.memoryPreloaded !== undefined) {
        console.log(`   💾 Memory preloaded: ${transition.metadata.memoryPreloaded}`);
      }

      if (transition.metadata.contextPreserved !== undefined) {
        console.log(`   🔗 Context preserved: ${transition.metadata.contextPreserved}`);
      }
    }

    console.log('=' .repeat(60));
  }

  findContextLossEvents(sessionId: string): SessionStateTransition[] {
    const history = this.auditTrail.getHistory(sessionId);
    return history.filter(t =>
      t.reason === 'recall' &&
      t.metadata.contextPreserved === false
    );
  }

  private getStateIcon(state: SessionState): string {
    const icons: Record<SessionState, string> = {
      initializing: '🚀',
      active: '✅',
      paused: '⏸️',
      resetting: '🔄',
      recalling: '🔙',
      terminating: '🛑',
      terminated: '💀',
    };
    return icons[state] || '❓';
  }
}
```

### 2. CLI Command

```bash
# View session history
openclaw debug:session-history <session-id>

# Find context loss events
openclaw debug:find-context-loss --since="24h ago"

# Export audit trail
openclaw debug:export-audit --format=json --output=audit.json
```

---

## References

- Source Report: `night_cycle_20260412_0315.md`
- Related Commit: 4d0f555 (startup context)
- Related Commit: c31aa6da (active memory context preservation)
- Related Pattern: `session_state_management_patterns.md`
