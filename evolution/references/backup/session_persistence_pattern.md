# Session Persistence Pattern

**Source:** OpenEvolve Night Cycle Report 2026-04-12 05:00  
**Author:** Pattern identified from issue #64687  
**Priority:** P0 - High Priority  
**Classification:** Architecture Pattern / State Management

---

## Problem Statement

**Gateway Restart Session Loss:** When OpenClaw Gateway restarts:
- All active sessions are lost
- Users must re-establish context
- In-flight operations interrupted
- Conversational continuity broken

**Impact:** Poor user experience during deployments, crashes, or planned restarts.

---

## Solution: Session Persistence with TTL

### Core Concept

Serialize active sessions to disk before shutdown, restore on startup:

```
Lifecycle:

Running Sessions → [Pre-Shutdown] → Serialize to Disk
                              ↓
                    [Gateway Restart]
                              ↓
Disk Storage → [Post-Startup] → Rehydrate Sessions
                              ↓
                    Active Sessions (with TTL validation)
```

### Implementation

#### 1. Session Snapshot Structure

```typescript
// src/session/session-snapshot.ts

export interface SessionSnapshot {
  // Session identification
  sessionId: string;
  sessionKey: string;
  
  // Metadata
  createdAt: Date;
  lastActivityAt: Date;
  expiresAt: Date;
  
  // Context
  channelId: string;
  channelType: string;
  userId?: string;
  
  // State
  messageHistory: Message[];
  toolState: Record<string, unknown>;
  agentConfig: AgentConfigSnapshot;
  
  // Serialization version for migrations
  version: number;
}

export interface AgentConfigSnapshot {
  agentId: string;
  model: string;
  systemPrompt?: string;
  temperature?: number;
  maxTokens?: number;
}

export interface Message {
  role: 'system' | 'user' | 'assistant' | 'tool';
  content: string;
  timestamp: Date;
  toolCalls?: ToolCall[];
  toolResults?: ToolResult[];
}

// Snapshot with metadata
export interface PersistedSnapshot {
  snapshot: SessionSnapshot;
  persistedAt: Date;
  checksum: string;  // Integrity verification
}
```

#### 2. Session Store with Persistence

```typescript
// src/session/persistent-session-store.ts

export class PersistentSessionStore {
  private sessions: Map<string, Session> = new Map();
  private persistencePath: string;
  private autoSaveInterval: NodeJS.Timeout | null = null;

  constructor(config: PersistentStoreConfig) {
    this.persistencePath = config.persistencePath ?? './data/sessions';
    
    // Ensure directory exists
    ensureDirSync(this.persistencePath);
    
    // Setup auto-save
    if (config.autoSaveMs) {
      this.autoSaveInterval = setInterval(
        () => this.persistAll(),
        config.autoSaveMs
      );
    }
    
    // Setup graceful shutdown hooks
    this.setupShutdownHooks();
  }

  /**
   * Restore sessions from disk on startup
   */
  async restoreSessions(): Promise<Session[]> {
    const snapshotFiles = await glob(`${this.persistencePath}/*.json`);
    const restored: Session[] = [];
    const expired: string[] = [];

    for (const file of snapshotFiles) {
      try {
        const content = await readFile(file, 'utf8');
        const persisted: PersistedSnapshot = JSON.parse(content);
        
        // Verify checksum
        if (!this.verifyChecksum(persisted)) {
          console.warn(`Session snapshot checksum invalid: ${file}`);
          continue;
        }

        // Check TTL
        if (this.isExpired(persisted.snapshot)) {
          expired.push(file);
          continue;
        }

        // Rehydrate session
        const session = this.rehydrateSession(persisted.snapshot);
        this.sessions.set(session.sessionId, session);
        restored.push(session);
        
      } catch (error) {
        console.error(`Failed to restore session from ${file}:`, error);
      }
    }

    // Clean up expired sessions
    await Promise.all(expired.map(f => unlink(f).catch(() => {})));

    console.info(`Restored ${restored.length} sessions, cleaned up ${expired.length} expired`);
    return restored;
  }

  /**
   * Persist all active sessions to disk
   */
  async persistAll(): Promise<void> {
    const promises: Promise<void>[] = [];

    for (const [sessionId, session] of this.sessions) {
      promises.push(this.persistSession(sessionId, session));
    }

    await Promise.all(promises);
  }

  /**
   * Persist single session
   */
  async persistSession(sessionId: string, session: Session): Promise<void> {
    const snapshot = this.createSnapshot(session);
    const persisted: PersistedSnapshot = {
      snapshot,
      persistedAt: new Date(),
      checksum: this.calculateChecksum(snapshot),
    };

    const filePath = join(this.persistencePath, `${sessionId}.json`);
    await writeFile(filePath, JSON.stringify(persisted, null, 2));
  }

  /**
   * Graceful shutdown persistence
   */
  async shutdown(): Promise<void> {
    console.info('Persisting sessions before shutdown...');
    
    if (this.autoSaveInterval) {
      clearInterval(this.autoSaveInterval);
    }

    await this.persistAll();
    
    console.info('Session persistence complete');
  }

  private createSnapshot(session: Session): SessionSnapshot {
    return {
      sessionId: session.sessionId,
      sessionKey: session.sessionKey,
      createdAt: session.createdAt,
      lastActivityAt: session.lastActivityAt,
      expiresAt: this.calculateExpiry(session),
      channelId: session.channelId,
      channelType: session.channelType,
      userId: session.userId,
      messageHistory: session.messageHistory.slice(-50),  // Last 50 messages
      toolState: session.toolState,
      agentConfig: {
        agentId: session.agentConfig.agentId,
        model: session.agentConfig.model,
        systemPrompt: session.agentConfig.systemPrompt,
        temperature: session.agentConfig.temperature,
        maxTokens: session.agentConfig.maxTokens,
      },
      version: 1,
    };
  }

  private rehydrateSession(snapshot: SessionSnapshot): Session {
    return new Session({
      sessionId: snapshot.sessionId,
      sessionKey: snapshot.sessionKey,
      channelId: snapshot.channelId,
      channelType: snapshot.channelType,
      userId: snapshot.userId,
      agentConfig: snapshot.agentConfig,
      messageHistory: snapshot.messageHistory,
      toolState: snapshot.toolState,
      // Reset activity timer
      lastActivityAt: new Date(),
    });
  }

  private calculateExpiry(session: Session): Date {
    const ttlMs = session.config?.ttlMs ?? 24 * 60 * 60 * 1000;  // Default 24h
    return new Date(Date.now() + ttlMs);
  }

  private isExpired(snapshot: SessionSnapshot): boolean {
    return new Date() > new Date(snapshot.expiresAt);
  }

  private calculateChecksum(snapshot: SessionSnapshot): string {
    const content = JSON.stringify(snapshot);
    return createHash('sha256').update(content).digest('hex').slice(0, 16);
  }

  private verifyChecksum(persisted: PersistedSnapshot): boolean {
    const calculated = this.calculateChecksum(persisted.snapshot);
    return calculated === persisted.checksum;
  }

  private setupShutdownHooks(): void {
    // SIGTERM handler (Docker, systemd)
    process.on('SIGTERM', async () => {
      await this.shutdown();
      process.exit(0);
    });

    // SIGINT handler (Ctrl+C)
    process.on('SIGINT', async () => {
      await this.shutdown();
      process.exit(0);
    });

    // BeforeExit handler
    process.on('beforeExit', async () => {
      await this.shutdown();
    });
  }
}
```

#### 3. Circuit Breaker Integration

```typescript
// src/session/resilient-persistence.ts

import { CircuitBreaker } from '../resilience/circuit-breaker';

export class ResilientSessionPersistence {
  private circuitBreaker: CircuitBreaker;

  constructor(private store: PersistentSessionStore) {
    this.circuitBreaker = new CircuitBreaker({
      failureThreshold: 3,
      resetTimeout: 30000,
      onStateChange: (state) => {
        console.warn(`Persistence circuit breaker: ${state}`);
      },
    });
  }

  async persistWithCircuitBreaker(sessionId: string, session: Session): Promise<void> {
    return this.circuitBreaker.execute(
      async () => this.store.persistSession(sessionId, session),
      {
        fallback: async () => {
          // Log to memory queue for retry
          this.queueForRetry(sessionId, session);
        },
      }
    );
  }

  private queueForRetry(sessionId: string, session: Session): void {
    // Add to in-memory queue
    // Retry when circuit closes
  }
}
```

---

## Configuration

```yaml
# config.yaml
session:
  persistence:
    enabled: true
    path: "/data/sessions"
    auto-save-ms: 60000  # Save every minute
    ttl-hours: 24        # Session expiry
    
    # What to persist
    include:
      - message-history
      - tool-state
      - agent-config
    exclude:
      - large-attachments
      - temporary-files
    
    # Circuit breaker settings
    circuit-breaker:
      failure-threshold: 3
      reset-timeout-ms: 30000
```

---

## Migration Path

### Phase 1: Add Persistence Layer (Non-Breaking)

```typescript
// Wrap existing store
const legacyStore = new SessionStore();
const persistentStore = new PersistentSessionStore({
  delegate: legacyStore,
  persistencePath: './data/sessions',
});

// Existing code continues to work
await persistentStore.createSession(/* ... */);
```

### Phase 2: Enable Persistence (Config Flag)

```typescript
const store = config.session.persistence?.enabled
  ? new PersistentSessionStore(config.session.persistence)
  : new SessionStore();
```

### Phase 3: Default On (Major Version)

Make persistence the default with opt-out:

```yaml
session:
  persistence:
    enabled: true  # Default
    opt-out: false
```

---

## Security Considerations

### Data Encryption

```typescript
// Encrypt session snapshots
import { encrypt, decrypt } from '../crypto/aes';

export class EncryptedSessionStore extends PersistentSessionStore {
  private encryptionKey: Buffer;

  constructor(config: EncryptedStoreConfig) {
    super(config);
    this.encryptionKey = loadEncryptionKey(config.keyPath);
  }

  protected async persistSession(sessionId: string, session: Session): Promise<void> {
    const snapshot = this.createSnapshot(session);
    const encrypted = encrypt(JSON.stringify(snapshot), this.encryptionKey);
    
    const filePath = join(this.persistencePath, `${sessionId}.enc`);
    await writeFile(filePath, encrypted);
  }
}
```

### Access Control

```typescript
// Restrict session file permissions
import { chmod } from 'fs/promises';

async function secureSessionFile(filePath: string): Promise<void> {
  // Owner read/write only
  await chmod(filePath, 0o600);
}
```

---

## Monitoring

### Metrics

| Metric | Description |
|--------|-------------|
| `session_restore_count` | Sessions restored on startup |
| `session_persist_count` | Sessions persisted |
| `session_expire_count` | Sessions cleaned up (expired) |
| `session_persist_latency_ms` | Persistence latency |
| `session_persist_failures` | Persistence failures |

### Health Check

```typescript
// Health check endpoint
app.get('/health/sessions', async (req, res) => {
  const stats = await store.getStats();
  
  if (stats.persistenceFailureRate > 0.1) {
    return res.status(503).json({
      status: 'degraded',
      sessions: stats.activeCount,
      persistence: 'failing',
    });
  }
  
  res.json({
    status: 'healthy',
    sessions: stats.activeCount,
    persisted: stats.persistedCount,
  });
});
```

---

## Testing

```typescript
// test/session-persistence.test.ts

describe('PersistentSessionStore', () => {
  let store: PersistentSessionStore;
  let tempDir: string;

  beforeEach(async () => {
    tempDir = await mkdtemp(join(tmpdir(), 'sessions-'));
    store = new PersistentSessionStore({
      persistencePath: tempDir,
      autoSaveMs: 1000,
    });
  });

  afterEach(async () => {
    await store.shutdown();
    await rm(tempDir, { recursive: true });
  });

  it('should restore sessions after restart', async () => {
    // Create session
    const session = await store.createSession({
      channelId: 'test-channel',
      userId: 'test-user',
    });

    // Add message
    session.addMessage({ role: 'user', content: 'Hello' });
    
    // Persist
    await store.persistSession(session.sessionId, session);

    // Simulate restart
    const newStore = new PersistentSessionStore({
      persistencePath: tempDir,
    });
    const restored = await newStore.restoreSessions();

    // Verify
    expect(restored).toHaveLength(1);
    expect(restored[0].messageHistory).toHaveLength(1);
    expect(restored[0].messageHistory[0].content).toBe('Hello');
  });

  it('should expire old sessions', async () => {
    // Create expired snapshot manually
    const expiredSnapshot: PersistedSnapshot = {
      snapshot: {
        sessionId: 'expired',
        expiresAt: new Date(Date.now() - 1000),  // Already expired
        // ... other fields
      } as SessionSnapshot,
      persistedAt: new Date(Date.now() - 86400000),
      checksum: 'test',
    };

    await writeFile(
      join(tempDir, 'expired.json'),
      JSON.stringify(expiredSnapshot)
    );

    const restored = await store.restoreSessions();
    
    expect(restored).toHaveLength(0);
    // Verify file was cleaned up
    expect(await fileExists(join(tempDir, 'expired.json'))).toBe(false);
  });

  it('should verify checksums', async () => {
    // Create corrupted snapshot
    await writeFile(
      join(tempDir, 'corrupted.json'),
      JSON.stringify({
        snapshot: { sessionId: 'test' },
        checksum: 'invalid',
      })
    );

    const restored = await store.restoreSessions();
    
    expect(restored).toHaveLength(0);
  });
});
```

---

## Related Patterns

- **Startup Context Pattern**: `startup_context_pattern_v2.md`
- **Circuit Breaker Pattern**: `circuit_breaker_pattern.md`
- **SQLite Fallback Strategy**: `sqlite_fallback_strategy.md`

---

## References

- Night Cycle Report: `night_cycle_20260412_0500.md`
- GitHub Issue: #64687

---

*Generated by OpenEvolve Auto-Apply*  
*Classification: P0 High Priority State Management Pattern*  
*Credit: Session persistence proposal from Night Cycle analysis*
